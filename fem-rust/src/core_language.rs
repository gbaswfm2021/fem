//! Minimal FEM adapter to the platform's typed expression DLL interface.
use fullmeta_core::expression::AstNode;
use std::{cell::RefCell, collections::HashMap, ffi::{c_char, c_void, OsStr}, marker::PhantomData, os::windows::ffi::OsStrExt,
    path::PathBuf, rc::Rc, sync::OnceLock};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Number { pub re: f64, pub im: f64 }
type Compile = unsafe extern "C" fn(*const u8, usize, *const u8, usize, *mut u64) -> i32;
type Release = unsafe extern "C" fn(u64) -> i32;
type Counts = unsafe extern "C" fn(u64, *mut usize, *mut usize) -> i32;
type Symbol = unsafe extern "C" fn(u64, usize, *mut *const u8, *mut usize) -> i32;
type Operator = unsafe extern "C" fn(u64, usize, *mut *const u8, *mut usize, *mut u64) -> i32;
type Evaluate = unsafe extern "C" fn(u64, usize, *const Number, usize, *const Number, usize, *mut Number, usize) -> i32;
type ErrorPointer = unsafe extern "C" fn() -> *const u8;
type ErrorLength = unsafe extern "C" fn() -> usize;
type Version = unsafe extern "C" fn() -> u32;
type ContextCompile = unsafe extern "C" fn(*const u8, usize, *mut u64) -> i32;
type ContextCount = unsafe extern "C" fn(u64, *mut usize) -> i32;
type ContextExpression = unsafe extern "C" fn(u64, usize, *mut *const u8, *mut usize, *mut u64) -> i32;
type ContextDefinitions = unsafe extern "C" fn(u64, *mut u64) -> i32;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

struct Api {
    _library: *mut c_void, compile: Compile, release: Release, counts: Counts, symbol: Symbol,
    operator: Operator, evaluate: Evaluate, error_pointer: ErrorPointer, error_length: ErrorLength,
    context_compile: ContextCompile, context_release: Release, context_count: ContextCount,
    context_expression: ContextExpression, context_definitions: ContextDefinitions,
}
// Export pointers never change; DLL handles and errors are thread-local.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

fn dll_path() -> Result<PathBuf, String> {
    Ok(fullmeta_core::library_path()?.to_path_buf())
}

impl Api {
    fn load() -> Result<Self, String> {
        let path = dll_path()?;
        let wide: Vec<u16> = OsStr::new(&path).encode_wide().chain(Some(0)).collect();
        let library = unsafe { LoadLibraryW(wide.as_ptr()) };
        if library.is_null() { return Err(format!("Cannot load core DLL {}: {}", path.display(), std::io::Error::last_os_error())); }
        let symbol = |name: &'static [u8]| -> Result<*mut c_void, String> {
            let pointer = unsafe { GetProcAddress(library, name.as_ptr().cast()) };
            if pointer.is_null() { Err(format!("Core DLL {} is missing {}", path.display(), String::from_utf8_lossy(&name[..name.len()-1]))) }
            else { Ok(pointer) }
        };
        let result = (|| {
            let version: Version = unsafe { std::mem::transmute(symbol(b"core_language_abi_version\0")?) };
            if unsafe { version() } != 1 { return Err("Unsupported core language ABI version.".into()); }
            Ok(Self {
                _library: library,
                compile: unsafe { std::mem::transmute::<*mut c_void, Compile>(symbol(b"core_language_compile_ast_json\0")?) },
                release: unsafe { std::mem::transmute::<*mut c_void, Release>(symbol(b"core_language_release\0")?) },
                counts: unsafe { std::mem::transmute::<*mut c_void, Counts>(symbol(b"core_language_counts\0")?) },
                symbol: unsafe { std::mem::transmute::<*mut c_void, Symbol>(symbol(b"core_language_symbol\0")?) },
                operator: unsafe { std::mem::transmute::<*mut c_void, Operator>(symbol(b"core_language_operator\0")?) },
                evaluate: unsafe { std::mem::transmute::<*mut c_void, Evaluate>(symbol(b"core_language_evaluate_batch\0")?) },
                error_pointer: unsafe { std::mem::transmute::<*mut c_void, ErrorPointer>(symbol(b"core_language_error_pointer\0")?) },
                error_length: unsafe { std::mem::transmute::<*mut c_void, ErrorLength>(symbol(b"core_language_error_length\0")?) },
                context_compile: unsafe { std::mem::transmute::<*mut c_void, ContextCompile>(symbol(b"core_context_compile\0")?) },
                context_release: unsafe { std::mem::transmute::<*mut c_void, Release>(symbol(b"core_context_release\0")?) },
                context_count: unsafe { std::mem::transmute::<*mut c_void, ContextCount>(symbol(b"core_context_count\0")?) },
                context_expression: unsafe { std::mem::transmute::<*mut c_void, ContextExpression>(symbol(b"core_context_expression\0")?) },
                context_definitions: unsafe { std::mem::transmute::<*mut c_void, ContextDefinitions>(symbol(b"core_context_definitions\0")?) },
            })
        })();
        if result.is_err() { unsafe { FreeLibrary(library); } }
        result
    }
    fn error(&self) -> String {
        let length = unsafe { (self.error_length)() };
        let pointer = unsafe { (self.error_pointer)() };
        if length == 0 || pointer.is_null() { return "Core expression operation failed.".into(); }
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(pointer, length) }).into_owned()
    }
    fn check(&self, result: i32) -> Result<(), String> { if result == 1 { Ok(()) } else { Err(self.error()) } }
}

fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(Api::load).as_ref().map_err(Clone::clone)
}

pub(crate) struct HostOperator { pub name: String, pub argument: Program }
pub(crate) struct Program {
    api: &'static Api, handle: u64, pub symbols: Vec<String>, pub operators: Vec<HostOperator>,
    _thread_local: PhantomData<Rc<()>>,
}
impl Drop for Program { fn drop(&mut self) { unsafe { (self.api.release)(self.handle); } } }
impl Program {
    pub fn compile(ast: &AstNode, names: &[String]) -> Result<Self, String> {
        let api = api()?;
        let ast = serde_json::to_vec(ast).map_err(|e| e.to_string())?;
        let names = serde_json::to_vec(names).map_err(|e| e.to_string())?;
        let mut handle = 0;
        api.check(unsafe { (api.compile)(ast.as_ptr(), ast.len(), names.as_ptr(), names.len(), &mut handle) })?;
        Self::from_handle(api, handle)
    }
    fn from_handle(api: &'static Api, handle: u64) -> Result<Self, String> {
        // This owner releases the handle even if reading metadata fails.
        let mut program = Self { api, handle, symbols: Vec::new(), operators: Vec::new(), _thread_local: PhantomData };
        let (mut symbols, mut operators) = (0, 0);
        api.check(unsafe { (api.counts)(handle, &mut symbols, &mut operators) })?;
        for index in 0..symbols {
            let (mut pointer, mut length) = (std::ptr::null(), 0);
            api.check(unsafe { (api.symbol)(handle, index, &mut pointer, &mut length) })?;
            program.symbols.push(read_name(pointer, length)?);
        }
        for index in 0..operators {
            let (mut pointer, mut length, mut child) = (std::ptr::null(), 0, 0);
            api.check(unsafe { (api.operator)(handle, index, &mut pointer, &mut length, &mut child) })?;
            let name = match read_name(pointer, length) { Ok(name) => name, Err(error) => { unsafe { (api.release)(child); } return Err(error); } };
            program.operators.push(HostOperator { name, argument: Self::from_handle(api, child)? });
        }
        Ok(program)
    }
    pub fn evaluate_batch(&self, rows: usize, symbols: &[Number], operators: &[Number]) -> Result<Vec<Number>, String> {
        let mut output = vec![Number::default(); rows];
        self.api.check(unsafe { (self.api.evaluate)(self.handle, rows, symbols.as_ptr(), symbols.len(),
            operators.as_ptr(), operators.len(), output.as_mut_ptr(), output.len()) })?;
        Ok(output)
    }
}
fn read_name(pointer: *const u8, length: usize) -> Result<String, String> {
    if pointer.is_null() || length > isize::MAX as usize { return Err("Core returned an invalid name.".into()); }
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(pointer, length) })
        .map(str::to_owned).map_err(|error| format!("Core returned a non-UTF-8 name: {error}"))
}

/// A DLL-owned compiled definition context reused on its creating thread.
pub(crate) struct Context {
    api: &'static Api, handle: u64,
    pub expressions: Vec<(String, Program)>,
    pub pointwise: Option<fullmeta_core::pointwise::DefinitionPlan>,
    _thread_local: PhantomData<Rc<()>>,
}
impl Drop for Context { fn drop(&mut self) { unsafe { (self.api.context_release)(self.handle); } } }
thread_local! { static CONTEXT_CACHE: RefCell<HashMap<Vec<u8>, Rc<Context>>> = RefCell::new(HashMap::new()); }
impl Context {
    pub fn compile(variables: &[(String, AstNode)], pointwise: &[(String, AstNode)], operators: &[String]) -> Result<Rc<Self>, String> {
        let data=serde_json::to_vec(&serde_json::json!({"variables":variables,"pointwise":pointwise,"operators":operators})).map_err(|e|e.to_string())?;
        if let Some(context)=CONTEXT_CACHE.with(|cache|cache.borrow().get(&data).cloned()){return Ok(context);}
        // Load both public interfaces before acquiring independent child handles.
        fullmeta_core::initialize()?;
        let api=api()?;let mut handle=0;
        api.check(unsafe{(api.context_compile)(data.as_ptr(),data.len(),&mut handle)})?;
        let mut context=Self{api,handle,expressions:Vec::new(),pointwise:None,_thread_local:PhantomData};
        let mut count=0;api.check(unsafe{(api.context_count)(handle,&mut count)})?;
        for index in 0..count {
            let (mut p,mut n,mut child)=(std::ptr::null(),0,0);
            api.check(unsafe{(api.context_expression)(handle,index,&mut p,&mut n,&mut child)})?;
            let name=match read_name(p,n){Ok(name)=>name,Err(error)=>{unsafe{(api.release)(child);}return Err(error);}};
            context.expressions.push((name,Program::from_handle(api,child)?));
        }
        let mut child=0;api.check(unsafe{(api.context_definitions)(handle,&mut child)})?;
        if child!=0 { context.pointwise=Some(unsafe{fullmeta_core::pointwise::DefinitionPlan::from_owned_handle(child)?}); }
        let context=Rc::new(context);
        CONTEXT_CACHE.with(|cache|{let mut cache=cache.borrow_mut();if cache.len()>=64{cache.clear();}cache.insert(data,context.clone());});
        Ok(context)
    }
}
