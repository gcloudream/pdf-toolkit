mod pdf;

use pdf::*;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::command;

// ── 辅助：生成临时输出路径 ──────────────────────────────

fn output_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("pdf-toolkit");
    fs::create_dir_all(&dir).ok();
    dir.join(format!("{}_{}", uuid_simple(), name))
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:x}", t)
}

// ── Tauri Commands ───────────────────────────────────────

#[derive(Serialize)]
struct MergeResult {
    output_path: String,
    total_pages: u32,
}

#[command]
fn cmd_pdf_info(path: String) -> Result<PdfInfo, String> {
    get_pdf_info(&PathBuf::from(path)).map_err(|e| e.to_string())
}

#[command]
fn cmd_merge_pdfs(paths: Vec<String>) -> Result<MergeResult, String> {
    let input: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let out = output_path("merged.pdf");
    merge_pdfs(&input, &out).map_err(|e| e.to_string())?;
    let info = get_pdf_info(&out).map_err(|e| e.to_string())?;
    Ok(MergeResult {
        output_path: out.to_string_lossy().into(),
        total_pages: info.pages,
    })
}

#[derive(Serialize)]
struct SplitResult {
    outputs: Vec<String>,
}

#[command]
fn cmd_split_pdf(path: String, mode: String, ranges: String, split_at: u32) -> Result<SplitResult, String> {
    let p = PathBuf::from(&path);
    let dir = std::env::temp_dir().join("pdf-toolkit").join(uuid_simple());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let outputs = match mode.as_str() {
        "each" => split_each(&p, &dir).map_err(|e| e.to_string())?,
        "range" => split_by_ranges(&p, &ranges, &dir).map_err(|e| e.to_string())?,
        "half" => split_at_page(&p, split_at, &dir).map_err(|e| e.to_string())?,
        _ => return Err(format!("unknown mode: {mode}")),
    };

    Ok(SplitResult {
        outputs: outputs.iter().map(|p| p.to_string_lossy().into()).collect(),
    })
}

#[derive(Serialize)]
struct DeleteResult {
    output_path: String,
    preview: DeletePreview,
}

#[command]
fn cmd_delete_preview(path: String, pages: String) -> Result<DeletePreview, String> {
    delete_preview_info(&PathBuf::from(&path), &pages).map_err(|e| e.to_string())
}

#[command]
fn cmd_delete_pages(path: String, pages: String) -> Result<DeleteResult, String> {
    let out = output_path("deleted.pdf");
    let preview = delete_pages(&PathBuf::from(&path), &pages, &out).map_err(|e| e.to_string())?;
    Ok(DeleteResult {
        output_path: out.to_string_lossy().into(),
        preview,
    })
}

#[derive(Serialize)]
struct ExtractResult {
    output_path: String,
    extracted_count: u32,
}

#[command]
fn cmd_extract_pages(path: String, pages: String) -> Result<ExtractResult, String> {
    let out = output_path("extracted.pdf");
    let count = extract_pages(&PathBuf::from(&path), &pages, &out).map_err(|e| e.to_string())?;
    Ok(ExtractResult {
        output_path: out.to_string_lossy().into(),
        extracted_count: count,
    })
}

#[derive(Serialize)]
struct RotateResult {
    output_path: String,
    rotated_count: u32,
}

#[command]
fn cmd_rotate_pages(path: String, pages: String, angle: i32) -> Result<RotateResult, String> {
    let out = output_path("rotated.pdf");
    let count = rotate_pages(&PathBuf::from(&path), &pages, angle, &out).map_err(|e| e.to_string())?;
    Ok(RotateResult {
        output_path: out.to_string_lossy().into(),
        rotated_count: count,
    })
}

#[derive(Serialize)]
struct ReorderResult {
    output_path: String,
}

#[command]
fn cmd_reorder_pages(path: String, order: String) -> Result<ReorderResult, String> {
    let out = output_path("reordered.pdf");
    reorder_pages(&PathBuf::from(&path), &order, &out).map_err(|e| e.to_string())?;
    Ok(ReorderResult {
        output_path: out.to_string_lossy().into(),
    })
}

#[derive(Serialize)]
struct CompressResult {
    output_path: String,
    original_bytes: u64,
    compressed_bytes: u64,
    ratio: String,
}

#[command]
fn cmd_compress_pdf(path: String) -> Result<CompressResult, String> {
    let out = output_path("compressed.pdf");
    let (orig, comp) = compress_pdf(&PathBuf::from(&path), &out).map_err(|e| e.to_string())?;
    let ratio = if orig > 0 { (1.0 - comp as f64 / orig as f64) * 100.0 } else { 0.0 };
    Ok(CompressResult {
        output_path: out.to_string_lossy().into(),
        original_bytes: orig,
        compressed_bytes: comp,
        ratio: format!("{ratio:.1}%"),
    })
}

#[command]
fn cmd_copy_file(src: String, dest: String) -> Result<(), String> {
    fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

// ── App Setup ────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            cmd_pdf_info,
            cmd_merge_pdfs,
            cmd_split_pdf,
            cmd_delete_preview,
            cmd_delete_pages,
            cmd_extract_pages,
            cmd_rotate_pages,
            cmd_reorder_pages,
            cmd_compress_pdf,
            cmd_copy_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
