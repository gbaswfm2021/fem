//! Loading the Gmsh C API from the application-provided SDK.
use std::ffi::{c_char, c_int, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> c_int;
}

type Initialize = unsafe extern "C" fn(c_int, *mut *mut c_char, c_int, c_int, *mut c_int);
type Finalize = unsafe extern "C" fn(*mut c_int);
type IsInitialized = unsafe extern "C" fn(*mut c_int) -> c_int;
type Open = unsafe extern "C" fn(*const c_char, *mut c_int);
type SetNumber = unsafe extern "C" fn(*const c_char, f64, *mut c_int);
type Bounds = unsafe extern "C" fn(
    c_int,
    c_int,
    *mut *mut f64,
    *mut usize,
    *mut *mut f64,
    *mut usize,
    *mut c_int,
);
type GetValue =
    unsafe extern "C" fn(c_int, c_int, *const f64, usize, *mut *mut f64, *mut usize, *mut c_int);
type IsInside = unsafe extern "C" fn(c_int, c_int, *const f64, usize, c_int, *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);
type PhysicalEntities = unsafe extern "C" fn(c_int, c_int, *mut *mut c_int, *mut usize, *mut c_int);
type PhysicalGroups = unsafe extern "C" fn(*mut *mut c_int, *mut usize, c_int, *mut c_int);
type RemovePhysicalGroups = unsafe extern "C" fn(*const c_int, usize, *mut c_int);
type AddPhysicalGroup =
    unsafe extern "C" fn(c_int, *const c_int, usize, c_int, *const c_char, *mut c_int) -> c_int;

pub struct GmshApi {
    module: *mut c_void,
    finalize: Finalize,
    is_initialized: IsInitialized,
}

impl GmshApi {
    fn symbol(&self, name: &'static [u8]) -> Result<*mut c_void, String> {
        let value = unsafe { GetProcAddress(self.module, name.as_ptr().cast()) };
        if value.is_null() {
            Err(format!(
                "Missing Gmsh API entry point: {}",
                String::from_utf8_lossy(&name[..name.len() - 1])
            ))
        } else {
            Ok(value)
        }
    }

    pub fn set_number(&self, name: &str, value: f64) -> Result<(), String> {
        let set: SetNumber = unsafe { std::mem::transmute(self.symbol(b"gmshOptionSetNumber\0")?) };
        let name = std::ffi::CString::new(name).map_err(|_| "Invalid Gmsh option name.")?;
        let mut error = 0;
        unsafe { set(name.as_ptr(), value, &mut error) };
        if error != 0 {
            Err(format!("Could not set Gmsh option: {error}."))
        } else {
            Ok(())
        }
    }

    pub fn open(&self, path: &Path) -> Result<(), String> {
        let name = std::ffi::CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| "Invalid Gmsh file path.")?;
        let open: Open = unsafe { std::mem::transmute(self.symbol(b"gmshOpen\0")?) };
        let mut error = 0;
        unsafe { open(name.as_ptr(), &mut error) };
        if error != 0 {
            Err(format!("Gmsh could not open {}: {error}.", path.display()))
        } else {
            Ok(())
        }
    }

    /// Read the entities already selected by the address-generated physical group.
    pub fn physical_entities(&self, dimension: i32, tag: i32) -> Result<Vec<i32>, String> {
        let groups: PhysicalGroups =
            unsafe { std::mem::transmute(self.symbol(b"gmshModelGetPhysicalGroups\0")?) };
        let get: PhysicalEntities =
            unsafe { std::mem::transmute(self.symbol(b"gmshModelGetEntitiesForPhysicalGroup\0")?) };
        let free: Free = unsafe { std::mem::transmute(self.symbol(b"gmshFree\0")?) };
        let (mut group_ptr, mut group_len, mut group_error) = (std::ptr::null_mut(), 0, 0);
        unsafe { groups(&mut group_ptr, &mut group_len, dimension, &mut group_error) };
        if group_error != 0 || (group_ptr.is_null() && group_len != 0) || group_len % 2 != 0 {
            if !group_ptr.is_null() {
                unsafe { free(group_ptr.cast()) }
            }
            return Err(format!(
                "Could not list physical groups in dimension {dimension}."
            ));
        }
        let present = group_len != 0
            && unsafe { std::slice::from_raw_parts(group_ptr, group_len) }
                .chunks_exact(2)
                .any(|pair| pair == [dimension, tag]);
        if !group_ptr.is_null() {
            unsafe { free(group_ptr.cast()) }
        }
        // Gmsh does not create a physical group for an empty address result.
        if !present {
            return Ok(Vec::new());
        }
        let (mut ptr, mut len, mut error) = (std::ptr::null_mut(), 0, 0);
        unsafe { get(dimension, tag, &mut ptr, &mut len, &mut error) };
        if error != 0 {
            if !ptr.is_null() {
                unsafe { free(ptr.cast()) }
            }
            return Err(format!(
                "Could not read physical group {dimension}:{tag}: {error}."
            ));
        }
        if len == 0 {
            if !ptr.is_null() {
                unsafe { free(ptr.cast()) }
            }
            return Ok(vec![]);
        }
        if ptr.is_null() {
            return Err("Gmsh returned a null entity list.".into());
        }
        let entities = unsafe { std::slice::from_raw_parts(ptr, len).to_vec() };
        unsafe { free(ptr.cast()) };
        Ok(entities)
    }

    /// Filter an already address-selected physical group by the zero expression.
    pub fn filter_physical_entities(
        &self,
        entity_dimension: i32,
        physical_tag: i32,
        model_dimension: usize,
        expression: &str,
        symbols: &std::collections::HashMap<String, crate::expression::Scalar>,
        tolerance: f64,
        path: &str,
    ) -> Result<Vec<i32>, String> {
        let mut selected = Vec::new();
        for tag in self.physical_entities(entity_dimension, physical_tag)? {
            let point = self.representative_point(entity_dimension, tag)?;
            let value = crate::selection_address::evaluate_coordinate_expression(
                expression,
                symbols,
                model_dimension,
                point,
                path,
            )?;
            if !value.is_finite() {
                return Err(format!(
                    "{path}: zero expression is not finite on entity {entity_dimension}:{tag}."
                ));
            }
            if value.abs() <= tolerance {
                selected.push(tag);
            }
        }
        Ok(selected)
    }

    /// Replace one candidate group after filtering, retaining its dimension and tag.
    pub fn replace_physical_group(
        &self,
        dimension: i32,
        tag: i32,
        name: &str,
        entities: &[i32],
    ) -> Result<(), String> {
        if entities.is_empty() {
            return Err(format!(
                "The zero expression selected no entities in physical group {dimension}:{tag}."
            ));
        }
        let remove: RemovePhysicalGroups =
            unsafe { std::mem::transmute(self.symbol(b"gmshModelRemovePhysicalGroups\0")?) };
        let add: AddPhysicalGroup =
            unsafe { std::mem::transmute(self.symbol(b"gmshModelAddPhysicalGroup\0")?) };
        let name = std::ffi::CString::new(name)
            .map_err(|_| "Physical group name contains a null byte.")?;
        let mut error = 0;
        unsafe { remove([dimension, tag].as_ptr(), 2, &mut error) };
        if error != 0 {
            return Err(format!(
                "Could not remove physical group {dimension}:{tag}: {error}."
            ));
        }
        let actual = unsafe {
            add(
                dimension,
                entities.as_ptr(),
                entities.len(),
                tag,
                name.as_ptr(),
                &mut error,
            )
        };
        if error != 0 || actual != tag {
            return Err(format!(
                "Could not replace physical group {dimension}:{tag}: {error}."
            ));
        }
        Ok(())
    }

    /// Return a point on the trimmed entity; never use its potentially off-surface centre of mass.
    pub fn representative_point(&self, dimension: i32, tag: i32) -> Result<[f64; 3], String> {
        if !(0..=2).contains(&dimension) {
            return Err("A point, curve, or surface is required.".into());
        }
        let bounds: Bounds =
            unsafe { std::mem::transmute(self.symbol(b"gmshModelGetParametrizationBounds\0")?) };
        let get: GetValue = unsafe { std::mem::transmute(self.symbol(b"gmshModelGetValue\0")?) };
        let inside: IsInside = unsafe { std::mem::transmute(self.symbol(b"gmshModelIsInside\0")?) };
        let free: Free = unsafe { std::mem::transmute(self.symbol(b"gmshFree\0")?) };
        let (mut lo, mut hi) = (std::ptr::null_mut(), std::ptr::null_mut());
        let (mut ln, mut hn, mut error) = (0, 0, 0);
        let (min, max) = if dimension == 0 {
            (vec![], vec![])
        } else {
            unsafe {
                bounds(
                    dimension, tag, &mut lo, &mut ln, &mut hi, &mut hn, &mut error,
                )
            };
            if error != 0 || lo.is_null() || hi.is_null() || ln != dimension as usize || hn != ln {
                if !lo.is_null() {
                    unsafe { free(lo.cast()) }
                }
                if !hi.is_null() {
                    unsafe { free(hi.cast()) }
                }
                return Err(format!(
                    "Could not read parameter bounds for entity {dimension}:{tag}."
                ));
            }
            let a = unsafe { std::slice::from_raw_parts(lo, ln).to_vec() };
            let b = unsafe { std::slice::from_raw_parts(hi, hn).to_vec() };
            unsafe {
                free(lo.cast());
                free(hi.cast());
            }
            (a, b)
        };
        for u in [0.5, 0.25, 0.75, 0.1, 0.9] {
            for v in [0.5, 0.25, 0.75, 0.1, 0.9] {
                let param: Vec<f64> = (0..dimension as usize)
                    .map(|i| min[i] + (max[i] - min[i]) * if i == 0 { u } else { v })
                    .collect();
                error = 0;
                if dimension > 0 {
                    let count = unsafe {
                        inside(dimension, tag, param.as_ptr(), param.len(), 1, &mut error)
                    };
                    if error != 0 {
                        return Err(format!(
                            "Could not check trimmed entity {dimension}:{tag}: {error}."
                        ));
                    }
                    if count == 0 {
                        continue;
                    }
                }
                let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
                unsafe {
                    get(
                        dimension,
                        tag,
                        param.as_ptr(),
                        param.len(),
                        &mut ptr,
                        &mut len,
                        &mut error,
                    )
                };
                if error == 0 && !ptr.is_null() && len == 3 {
                    let result = unsafe { std::slice::from_raw_parts(ptr, len) }
                        .try_into()
                        .unwrap();
                    unsafe { free(ptr.cast()) };
                    return Ok(result);
                }
                if !ptr.is_null() {
                    unsafe { free(ptr.cast()) }
                }
            }
        }
        Err(format!(
            "Could not find a verified point on entity {dimension}:{tag}."
        ))
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(format!(
                "Could not load Gmsh API library: {}",
                path.display()
            ));
        }
        let symbol = |name: &'static [u8]| unsafe { GetProcAddress(module, name.as_ptr().cast()) };
        let init = symbol(b"gmshInitialize\0");
        let finalize = symbol(b"gmshFinalize\0");
        let initialized = symbol(b"gmshIsInitialized\0");
        if init.is_null() || finalize.is_null() || initialized.is_null() {
            unsafe { FreeLibrary(module) };
            return Err("Gmsh API library is missing required entry points.".into());
        }
        let init: Initialize = unsafe { std::mem::transmute(init) };
        let finalize: Finalize = unsafe { std::mem::transmute(finalize) };
        let is_initialized: IsInitialized = unsafe { std::mem::transmute(initialized) };
        let mut error = 0;
        unsafe { init(0, std::ptr::null_mut(), 0, 0, &mut error) };
        if error != 0 {
            unsafe { FreeLibrary(module) };
            return Err(format!("Gmsh API initialization failed: {error}."));
        }
        Ok(Self {
            module,
            finalize,
            is_initialized,
        })
    }

    pub fn initialized(&self) -> Result<bool, String> {
        let mut error = 0;
        let value = unsafe { (self.is_initialized)(&mut error) };
        if error != 0 {
            Err(format!("Gmsh API status failed: {error}."))
        } else {
            Ok(value != 0)
        }
    }
}

impl Drop for GmshApi {
    fn drop(&mut self) {
        let mut error = 0;
        unsafe {
            (self.finalize)(&mut error);
            FreeLibrary(self.module);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GmshApi;
    #[test]
    fn loads_the_bundled_gmsh_api() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/gmsh-4.15.2-sdk/gmsh-4.15.dll");
        let api = GmshApi::load(&path).expect("load the Gmsh SDK library");
        assert!(api.initialized().expect("read initialization status"));
        let geo =
            std::env::temp_dir().join(format!("fullmeta-gmsh-api-{}.geo", std::process::id()));
        std::fs::write(
            &geo,
            "SetFactory(\"OpenCASCADE\"); Circle(1) = {0, 0, 0, 2}; Physical Curve(7) = {1};\n",
        )
        .unwrap();
        api.open(&geo).expect("open the circle");
        assert_eq!(
            api.physical_entities(1, 7)
                .expect("read the address-selected entities"),
            vec![1]
        );
        api.replace_physical_group(1, 7, "contour", &[1])
            .expect("replace the filtered group");
        assert_eq!(api.physical_entities(1, 7).unwrap(), vec![1]);
        let p = api.representative_point(1, 1).expect("point on the circle");
        assert!((p[0].hypot(p[1]) - 2.0).abs() < 1e-8);
        std::fs::remove_file(geo).unwrap();
    }
}
