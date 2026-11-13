//! FEM-owned implementation of the schema-registered figure-save API.
//! The Kernel server only dispatches this JSON-over-stdio request.

use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::result_resolve;

type Result<T> = std::result::Result<T, String>;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
static SAVE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn save(input: &Value) -> Result<Value> {
    let root = env::var("FULLMETA_ROOT")
        .map(PathBuf::from)
        .map_err(|_| "The Kernel server did not provide the FullMeta root.".to_string())?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the FullMeta root: {error}"))?;
    let document = env::var("FULLMETA_DOCUMENT")
        .map(PathBuf::from)
        .map_err(|_| "The figure-save request must identify its opened document.".to_string())?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the opened document: {error}"))?;
    if !document.starts_with(&root) || !document.is_file() {
        return Err("The opened document must be a regular file inside FullMeta.".into());
    }
    let model = input
        .get("model")
        .filter(|value| value.is_object())
        .ok_or("The figure-save request must contain the current model JSON.")?;
    let validation_errors = crate::model_validation::validate(model);
    if !validation_errors.is_empty() {
        return Err(validation_errors.join("\n"));
    }
    if let Some(message) = crate::sibling_model_id_conflict(model, &document)? {
        return Err(message);
    }

    let model_id = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let model_folder = input
        .get("folder")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("projects/{model_id}"));
    let project = result_resolve::checked_existing_path(&root, &model_folder, true)?;
    if document.parent() != Some(project.as_path()) {
        return Err(
            "The figure-save request folder does not match the opened model document.".into(),
        );
    }
    if let Some(file) = input.get("file").and_then(Value::as_str) {
        if document.file_name().and_then(|name| name.to_str()) != Some(file) {
            return Err(
                "The figure-save request file does not match the opened model document.".into(),
            );
        }
    }
    let data_folder = input
        .get("dataFolder")
        .and_then(Value::as_str)
        .unwrap_or(&model_folder);
    if result_resolve::checked_existing_path(&root, data_folder, true)? != project {
        return Err("Result output must stay beside the selected model JSON.".into());
    }

    let action_id = input
        .get("actionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or("The figure-save request must select an action ID.")?;
    let action = model
        .pointer("/model/results")
        .and_then(Value::as_array)
        .and_then(|actions| {
            actions
                .iter()
                .find(|action| action.get("id").and_then(Value::as_str) == Some(action_id))
        })
        .ok_or_else(|| format!("Unknown result action {action_id}."))?;
    if action.get("type").and_then(Value::as_str) != Some("plot.sweep") {
        return Err(format!(
            "Saving figures is not supported for result action type {}.",
            action
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("(unknown)")
        ));
    }

    let directory_value = action
        .pointer("/output/directory")
        .and_then(Value::as_str)
        .unwrap_or("");
    let output_segments = directory_segments(directory_value)?;
    let figure_filename = action
        .pointer("/output/figureFilename")
        .and_then(Value::as_str)
        .unwrap_or("");
    let extension = figure_extension(figure_filename)?;
    validate_datasets(model, action, &root, &document, &project)?;
    let output_directory = create_output_directory(&project, &output_segments)?;
    let output_path = output_directory.join(figure_filename);
    match fs::symlink_metadata(&output_path) {
        Ok(info) if is_link(&info) => return Err("Result output path is a symbolic link.".into()),
        Ok(info) if !info.is_file() => {
            return Err("The configured result output path is not a regular file.".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect result output file: {error}")),
    }

    let image_width = input
        .get("imageWidth")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or("The rendered result image dimensions are invalid.")?;
    let image_height = input
        .get("imageHeight")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or("The rendered result image dimensions are invalid.")?;
    let (figure_bytes, _, _) = figure_bytes(
        action,
        figure_filename,
        extension,
        input
            .get("imageDataUrl")
            .and_then(Value::as_str)
            .unwrap_or(""),
        image_width,
        image_height,
    )?;
    let save_id = next_save_id();
    write_atomically(&output_path, &figure_bytes, &save_id)?;

    Ok(json!({
        "actionRunId": save_id,
        "state": "completed",
        "figure": format!("{}/{}", output_segments.join("/"), figure_filename),
        "bytes": figure_bytes.len()
    }))
}

fn validate_datasets(
    model: &Value,
    action: &Value,
    root: &Path,
    document: &Path,
    project: &Path,
) -> Result<()> {
    let runs_name = env::var("FULLMETA_STORAGE_RUNS_DIRECTORY")
        .map_err(|_| "The active schema does not declare its run storage directory.".to_string())?;
    if !result_resolve::valid_single_component(&runs_name) {
        return Err("The active schema declares an invalid run storage directory.".into());
    }
    let runs_directory = project.join(runs_name);
    let resolved =
        result_resolve::resolve_plot_datasets(action, model, root, document, &runs_directory)?;
    let datasets = resolved
        .get("datasets")
        .and_then(Value::as_array)
        .ok_or("The selected result action has no datasets.")?;
    let series = action
        .get("series")
        .and_then(Value::as_array)
        .ok_or("The selected result action has no series.")?;
    for dataset in datasets {
        if dataset["missing"] == true {
            return Err(dataset["message"]
                .as_str()
                .unwrap_or("The selected dataset is unavailable.")
                .to_string());
        }
        // The resolver has already checked Study CSV and its saved coordinate metadata.
        if dataset.pointer("/source/type").and_then(Value::as_str) == Some("study") {
            continue;
        }
        let relative = dataset
            .get("path")
            .and_then(Value::as_str)
            .ok_or("A file result source has no path.")?;
        let csv_path = result_resolve::checked_file_maybe(project, relative)?
            .ok_or_else(|| format!("Result CSV is missing: {relative}"))?;
        let csv = fs::read_to_string(&csv_path).map_err(|error| {
            format!("Could not read result CSV {}: {error}", csv_path.display())
        })?;
        let headers = csv_header(&csv)?;
        let mut required = dataset
            .get("xColumn")
            .and_then(Value::as_str)
            .into_iter()
            .chain(
                series
                    .iter()
                    .filter_map(|row| row.get("column").and_then(Value::as_str)),
            )
            .chain(
                dataset
                    .get("parameterColumns")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            )
            .chain(
                action
                    .get("tableColumns")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            )
            .collect::<Vec<_>>();
        required.sort();
        required.dedup();
        let missing = required
            .into_iter()
            .filter(|column| !headers.iter().any(|header| header == column))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "Dataset {} is missing CSV column{}: {}.",
                dataset
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("(unnamed)"),
                if missing.len() == 1 { "" } else { "s" },
                missing.join(", ")
            ));
        }
    }
    Ok(())
}

fn csv_header(csv: &str) -> Result<Vec<String>> {
    let mut chars = csv.trim_start_matches('\u{feff}').chars().peekable();
    let mut headers = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut closed = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' && chars.peek() == Some(&'"') {
                chars.next();
                cell.push('"');
            } else if character == '"' {
                quoted = false;
                closed = true;
            } else {
                cell.push(character);
            }
        } else if character == ',' || character == '\r' || character == '\n' {
            headers.push(cell.trim().to_owned());
            cell.clear();
            closed = false;
            if character != ',' {
                break;
            }
        } else if character == '"' && cell.is_empty() && !closed {
            quoted = true;
        } else if character == '"' || (closed && !character.is_whitespace()) {
            return Err("Result CSV has an invalid header.".into());
        } else {
            cell.push(character);
        }
        if chars.peek().is_none() {
            headers.push(cell.trim().to_owned());
        }
    }
    let unique = headers.iter().collect::<std::collections::HashSet<_>>();
    if quoted
        || headers.is_empty()
        || headers.iter().any(String::is_empty)
        || unique.len() != headers.len()
    {
        return Err("Result CSV has an invalid header.".into());
    }
    Ok(headers)
}

#[cfg(test)]
#[test]
fn file_export_reads_literal_quoted_column_names() {
    assert_eq!(
        csv_header("x,\"phase, degrees\",\"a\"\"b\"\r\n1,2,3\r\n").unwrap(),
        ["x", "phase, degrees", "a\"b"]
    );
    assert!(csv_header("x,\"x\"\n1,2\n").is_err());
}

fn directory_segments(value: &str) -> Result<Vec<String>> {
    let normalized = value.replace('\\', "/");
    let segments = normalized
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| !valid_output_component(segment, 64))
    {
        return Err(
            "Result output.directory must be a relative folder under the project data folder."
                .into(),
        );
    }
    Ok(segments)
}

fn valid_output_component(value: &str, maximum: usize) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && value.len() <= maximum
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

fn figure_extension(filename: &str) -> Result<&'static str> {
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("");
    let stem = filename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or("");
    if !valid_output_component(stem, 99) {
        return Err("Result output.figureFilename must be a .png, .svg, or .pdf filename.".into());
    }
    match extension.to_ascii_lowercase().as_str() {
        "png" => Ok("png"),
        "svg" => Ok("svg"),
        "pdf" => Ok("pdf"),
        _ => Err("Result output.figureFilename must end in .png, .svg, or .pdf.".into()),
    }
}

fn create_output_directory(project: &Path, segments: &[String]) -> Result<PathBuf> {
    let project = project
        .canonicalize()
        .map_err(|error| format!("Could not resolve project folder: {error}"))?;
    let mut current = project.clone();
    for segment in segments {
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(info) if is_link(&info) || !info.is_dir() => {
                return Err(
                    "Result output paths must use regular folders without symbolic links.".into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| format!("Could not create result output folder: {error}"))?;
            }
            Err(error) => return Err(format!("Could not inspect result output folder: {error}")),
        }
    }
    let actual = current
        .canonicalize()
        .map_err(|error| format!("Could not resolve result output folder: {error}"))?;
    if actual == project || !actual.starts_with(&project) {
        return Err("Result output resolves outside the project folder.".into());
    }
    Ok(actual)
}

fn figure_bytes(
    action: &Value,
    filename: &str,
    extension: &str,
    data_url: &str,
    supplied_width: u32,
    supplied_height: u32,
) -> Result<(Vec<u8>, u32, u32)> {
    if supplied_width == 0
        || supplied_height == 0
        || supplied_width > 10_000
        || supplied_height > 10_000
    {
        return Err("The rendered result image dimensions are invalid.".into());
    }
    let (image_type, encoded) = if let Some(data) = data_url.strip_prefix("data:image/png;base64,")
    {
        ("png", data)
    } else if let Some(data) = data_url.strip_prefix("data:image/jpeg;base64,") {
        ("jpeg", data)
    } else {
        return Err("The result image must be a base64 PNG or JPEG from the result viewer.".into());
    };
    let image = decode_base64(encoded)?;
    if image.is_empty() || image.len() > MAX_IMAGE_BYTES {
        return Err("The rendered result image must be between 1 byte and 8 MiB.".into());
    }
    let (mut width, mut height) = (supplied_width, supplied_height);
    if image_type == "png" {
        if image.len() < 24 || image.get(..8) != Some(&[137, 80, 78, 71, 13, 10, 26, 10]) {
            return Err("The rendered PNG is malformed.".into());
        }
        width = u32::from_be_bytes(
            image[16..20]
                .try_into()
                .map_err(|_| "The rendered PNG is malformed.")?,
        );
        height = u32::from_be_bytes(
            image[20..24]
                .try_into()
                .map_err(|_| "The rendered PNG is malformed.")?,
        );
        if width == 0 || height == 0 || width > 10_000 || height > 10_000 {
            return Err("The rendered result image dimensions are invalid.".into());
        }
    } else if image.get(..3) != Some(&[255, 216, 255]) {
        return Err("The rendered JPEG is malformed.".into());
    }
    let bytes = match extension {
        "png" if image_type == "png" => image,
        "png" => return Err("This result is configured to save as PNG.".into()),
        "svg" if image_type == "png" => {
            let encoded_image = encode_base64(&image);
            let title = xml_escape(
                action
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Result"),
            );
            format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><title>{title}</title><image width=\"{width}\" height=\"{height}\" href=\"data:image/png;base64,{encoded_image}\"/></svg>\n").into_bytes()
        }
        "svg" => {
            return Err("This result is configured to save as SVG and needs a PNG render.".into());
        }
        "pdf" if image_type == "jpeg" => make_pdf(&image, width, height),
        "pdf" => {
            return Err("This result is configured to save as PDF and needs a JPEG render.".into());
        }
        _ => {
            return Err(format!(
                "Result figureFilename must end in .png, .svg, or .pdf: {filename}"
            ));
        }
    };
    Ok((bytes, width, height))
}

fn decode_base64(value: &str) -> Result<Vec<u8>> {
    let bytes = value.as_bytes();
    let padding = bytes.iter().rev().take_while(|byte| **byte == b'=').count();
    if padding > 2 || bytes[..bytes.len().saturating_sub(padding)].contains(&b'=') {
        return Err("The result image has invalid base64 data.".into());
    }
    let data_len = bytes.len() - padding;
    let remainder = data_len % 4;
    if remainder == 1
        || (padding > 0
            && (bytes.len() % 4 != 0
                || (padding == 1 && remainder != 3)
                || (padding == 2 && remainder != 2)))
    {
        return Err("The result image has invalid base64 data.".into());
    }
    let mut output = Vec::with_capacity((data_len * 3) / 4);
    let (mut accumulator, mut bits) = (0_u32, 0_u32);
    for &byte in &bytes[..data_len] {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("The result image has invalid base64 data.".into()),
        } as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    if bits > 0 && accumulator & ((1 << bits) - 1) != 0 {
        return Err("The result image has invalid base64 data.".into());
    }
    Ok(output)
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn make_pdf(image: &[u8], width: u32, height: u32) -> Vec<u8> {
    let content = format!("q\n{width} 0 0 {height} 0 0 cm\n/Im0 Do\nQ\n").into_bytes();
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>").into_bytes(),
    ];
    let mut image_object = format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", image.len()).into_bytes();
    image_object.extend_from_slice(image);
    image_object.extend_from_slice(b"\nendstream");
    objects.push(image_object);
    let mut content_object = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    content_object.extend_from_slice(&content);
    content_object.extend_from_slice(b"endstream");
    objects.push(content_object);

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0_usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn next_save_id() -> String {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = SAVE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{milliseconds}-{}-{sequence}", std::process::id())
}

fn write_atomically(path: &Path, contents: &[u8], id: &str) -> Result<()> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("The configured figure filename is invalid.")?;
    let temporary = path.with_file_name(format!("{filename}.{id}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create temporary result figure: {error}"))?;
        file.write_all(contents)
            .map_err(|error| format!("Could not write result figure: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush result figure: {error}"))?;
        drop(file);
        replace_file(&temporary, path)
            .map_err(|error| format!("Could not install result figure: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

fn is_link(info: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        info.file_type().is_symlink() || info.file_attributes() & REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        info.file_type().is_symlink()
    }
}
