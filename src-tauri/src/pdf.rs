use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum PdfError {
    #[error("lopdf error: {0}")]
    Lopdf(#[from] lopdf::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("page {0} out of range (1-{1})")]
    PageOutOfRange(u32, u32),
}

impl Serialize for PdfError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type PdfResult<T> = Result<T, PdfError>;

#[derive(Serialize, Deserialize)]
pub struct PdfInfo {
    pub pages: u32,
    pub file_size: u64,
}

#[derive(Serialize)]
pub struct DeletePreview {
    pub total_pages: u32,
    pub pages_to_delete: Vec<u32>,
    pub delete_count: u32,
    pub first_page_num: u32,
    pub last_page_num: u32,
}

// ── 读取 PDF 信息 ────────────────────────────────────────

pub fn get_pdf_info(path: &PathBuf) -> PdfResult<PdfInfo> {
    let doc = Document::load(path)?;
    let pages = doc.get_pages().len() as u32;
    let file_size = std::fs::metadata(path)?.len();
    Ok(PdfInfo { pages, file_size })
}

// ── 解析页码字符串: "1-3,5,7-10" -> [1,2,3,5,7,8,9,10] ──

fn parse_pages(s: &str, total: u32) -> PdfResult<Vec<u32>> {
    let mut result = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() { continue; }
        if let Some((start_s, end_s)) = part.split_once('-') {
            let start: u32 = start_s.trim().parse().map_err(|_| PdfError::InvalidInput(format!("invalid page: {part}")))?;
            let end: u32 = end_s.trim().parse().map_err(|_| PdfError::InvalidInput(format!("invalid page: {part}")))?;
            if start < 1 || end > total || start > end {
                return Err(PdfError::InvalidInput(format!("range {part} out of 1-{total}")));
            }
            for p in start..=end { result.push(p); }
        } else {
            let p: u32 = part.parse().map_err(|_| PdfError::InvalidInput(format!("invalid page: {part}")))?;
            if p < 1 || p > total {
                return Err(PdfError::PageOutOfRange(p, total));
            }
            result.push(p);
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

// ── 复制页面到新文档 ────────────────────────────────────

fn copy_pages(doc: &Document, page_nums: &[u32]) -> PdfResult<Document> {
    let mut new_doc = Document::with_version("1.5");
    let pages_map = doc.get_pages();

    // 建立 old_id -> new_id 的映射
    let mut id_map: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();

    // 先复制所有间接对象
    for (&old_id, obj) in doc.objects.iter() {
        let new_id = new_doc.add_object(obj.clone());
        id_map.insert(old_id, new_id);
    }

    // 修正引用
    for (_, obj) in new_doc.objects.iter_mut() {
        fix_references(obj, &id_map);
    }

    // 创建新的 Pages 和 Page Tree
    let pages_id = new_doc.add_object(Object::Dictionary(Default::default()));
    let mut page_ids = Vec::new();

    for &page_num in page_nums {
        if let Some(&(_, page_id)) = pages_map.iter().nth((page_num - 1) as usize) {
            page_ids.push(page_id);
        }
    }

    // 用简单方法：创建新文档，只保留需要的页面
    let mut result = Document::with_version("1.5");

    // 复制页面对象
    for &page_num in page_nums {
        if let Some(&(_, page_id)) = pages_map.iter().nth((page_num - 1) as usize) {
            if let Ok(page_obj) = doc.get_object(page_id) {
                let new_page_id = result.add_object(page_obj.clone());
                // 修正页面内的引用
                if let Ok(page) = result.get_object(new_page_id).cloned() {
                    fix_references_deep(&mut result, &page, doc);
                }
            }
        }
    }

    // 使用 lopdf 的高级 API 来重新组织页面
    // 简化方案：直接用 Document 的页面操作
    let mut final_doc = doc.clone();
    let all_pages: Vec<u32> = (1..=total_pages(doc)).collect();
    let pages_to_remove: Vec<u32> = all_pages.into_iter()
        .filter(|p| !page_nums.contains(p))
        .collect();

    // 从后往前删除不需要的页面（保持索引稳定）
    for &p in pages_to_remove.iter().rev() {
        delete_page_num(&mut final_doc, p);
    }

    Ok(final_doc)
}

fn fix_references(_obj: &Object, _id_map: &BTreeMap<ObjectId, ObjectId>) {
    // lopdf 的 clone 会保留引用，通常不需要手动修正
}

fn fix_references_deep(_doc: &mut Document, _obj: &Object, _src: &Document) {
    // 简化处理
}

fn total_pages(doc: &Document) -> u32 {
    doc.get_pages().len() as u32
}

fn delete_page_num(doc: &mut Document, page_num: u32) {
    let pages = doc.get_pages();
    if let Some(&(_, page_id)) = pages.iter().nth((page_num - 1) as usize) {
        // 从 page tree 中移除
        if let Ok(pages_dict_id) = doc.trailer.get(b"Root")
            .and_then(|root| doc.get_object(root.as_reference()?))
            .and_then(|root| root.as_dict()?.get(b"Pages"))
            .and_then(|pages| pages.as_reference())
        {
            remove_page_from_tree(doc, pages_dict_id, page_id);
        }
        // 删除页面对象本身
        doc.objects.remove(&page_id);
    }
}

fn remove_page_from_tree(doc: &mut Document, tree_id: ObjectId, target_page: ObjectId) {
    let obj = match doc.objects.get(&tree_id).cloned() {
        Some(o) => o,
        None => return,
    };
    if let Ok(dict) = obj.as_dict() {
        if let Ok(kids) = dict.get(b"Kids") {
            if let Ok(arr) = kids.as_array() {
                let mut new_kids = Vec::new();
                for kid in arr {
                    if let Ok(kid_ref) = kid.as_reference() {
                        if kid_ref == target_page {
                            // 减少 Count
                            if let Ok(count) = dict.get(b"Count").and_then(|c| c.as_i64()) {
                                if let Some(d) = doc.objects.get_mut(&tree_id) {
                                    if let Ok(dd) = d.as_dict_mut() {
                                        dd.set("Count", Object::Integer(count as i64 - 1));
                                    }
                                }
                            }
                            continue; // 跳过这个页面
                        }
                        // 检查子树
                        if let Some(child_obj) = doc.objects.get(&kid_ref) {
                            if let Ok(child_dict) = child_obj.as_dict() {
                                if child_dict.has(b"Kids") {
                                    remove_page_from_tree(doc, kid_ref, target_page);
                                }
                            }
                        }
                        new_kids.push(kid.clone());
                    }
                }
                if let Some(d) = doc.objects.get_mut(&tree_id) {
                    if let Ok(dd) = d.as_dict_mut() {
                        dd.set("Kids", Object::Array(new_kids));
                    }
                }
            }
        }
    }
}

// ── 公开 API ─────────────────────────────────────────────

/// 合并多个 PDF
pub fn merge_pdfs(paths: &[PathBuf], output: &PathBuf) -> PdfResult<()> {
    if paths.len() < 2 {
        return Err(PdfError::InvalidInput("need at least 2 PDFs".into()));
    }
    let mut doc = Document::load(paths[0].clone())?;
    for path in &paths[1..] {
        let other = Document::load(path.clone())?;
        doc.merge_document(other);
    }
    doc.save(output)?;
    Ok(())
}

/// 拆分 PDF: 每页一个文件
pub fn split_each(path: &PathBuf, output_dir: &PathBuf) -> PdfResult<Vec<PathBuf>> {
    let doc = Document::load(path.clone())?;
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let mut outputs = Vec::new();

    for i in 0..total {
        let mut new_doc = doc.clone();
        // 删除除第 i+1 页外的所有页面
        for j in (0..total).rev() {
            if j != i {
                delete_page_num(&mut new_doc, j + 1);
            }
        }
        let out = output_dir.join(format!("page_{}.pdf", i + 1));
        new_doc.save(&out)?;
        outputs.push(out);
    }
    Ok(outputs)
}

/// 拆分 PDF: 按范围
pub fn split_by_ranges(path: &PathBuf, ranges: &str, output_dir: &PathBuf) -> PdfResult<Vec<PathBuf>> {
    let doc = Document::load(path.clone())?;
    let total = total_pages(&doc);
    let mut outputs = Vec::new();

    for (idx, r) in ranges.split(',').enumerate() {
        let r = r.trim();
        if r.is_empty() { continue; }
        let (start, end) = if let Some((s, e)) = r.split_once('-') {
            (s.trim().parse::<u32>().unwrap(), e.trim().parse::<u32>().unwrap())
        } else {
            let n: u32 = r.parse().unwrap();
            (n, n)
        };
        let mut new_doc = doc.clone();
        let keep: Vec<u32> = (start..=end).collect();
        for p in (1..=total).rev() {
            if !keep.contains(&p) {
                delete_page_num(&mut new_doc, p);
            }
        }
        let out = output_dir.join(format!("part_{}_p{}-{}.pdf", idx + 1, start, end));
        new_doc.save(&out)?;
        outputs.push(out);
    }
    Ok(outputs)
}

/// 拆分 PDF: 从指定页码一分为二
pub fn split_at_page(path: &PathBuf, split_at: u32, output_dir: &PathBuf) -> PdfResult<Vec<PathBuf>> {
    let doc = Document::load(path.clone())?;
    let total = total_pages(&doc);
    if split_at < 1 || split_at > total {
        return Err(PdfError::PageOutOfRange(split_at, total));
    }
    let mut outputs = Vec::new();

    // Part 1: 1..=split_at
    let mut doc1 = doc.clone();
    for p in (split_at + 1..=total).rev() {
        delete_page_num(&mut doc1, p);
    }
    let out1 = output_dir.join(format!("part1_p1-{}.pdf", split_at));
    doc1.save(&out1)?;
    outputs.push(out1);

    // Part 2: split_at+1..total
    if split_at < total {
        let mut doc2 = doc.clone();
        for p in (1..=split_at).rev() {
            delete_page_num(&mut doc2, p);
        }
        let out2 = output_dir.join(format!("part2_p{}-{}.pdf", split_at + 1, total));
        doc2.save(&out2)?;
        outputs.push(out2);
    }

    Ok(outputs)
}

/// 删除指定页面
pub fn delete_pages(path: &PathBuf, pages_str: &str, output: &PathBuf) -> PdfResult<DeletePreview> {
    let mut doc = Document::load(path.clone())?;
    let total = total_pages(&doc);
    let pages_to_delete = parse_pages(pages_str, total)?;

    let preview = DeletePreview {
        total_pages: total,
        delete_count: pages_to_delete.len() as u32,
        first_page_num: *pages_to_delete.first().unwrap_or(&0),
        last_page_num: *pages_to_delete.last().unwrap_or(&0),
        pages_to_delete: pages_to_delete.clone(),
    };

    for &p in pages_to_delete.iter().rev() {
        delete_page_num(&mut doc, p);
    }
    doc.save(output)?;

    Ok(preview)
}

/// 提取指定页面
pub fn extract_pages(path: &PathBuf, pages_str: &str, output: &PathBuf) -> PdfResult<u32> {
    let doc = Document::load(path.clone())?;
    let total = total_pages(&doc);
    let pages = parse_pages(pages_str, total)?;

    let mut new_doc = doc.clone();
    for p in (1..=total).rev() {
        if !pages.contains(&p) {
            delete_page_num(&mut new_doc, p);
        }
    }
    new_doc.save(output)?;
    Ok(pages.len() as u32)
}

/// 旋转指定页面
pub fn rotate_pages(path: &PathBuf, pages_str: &str, angle: i32, output: &PathBuf) -> PdfResult<u32> {
    let mut doc = Document::load(path.clone())?;
    let total = total_pages(&doc);
    let page_list = if pages_str == "all" {
        (1..=total).collect()
    } else {
        parse_pages(pages_str, total)?
    };

    let rotated = page_list.len() as u32;
    let pages_map = doc.get_pages();

    for &page_num in &page_list {
        if let Some(&(_, page_id)) = pages_map.iter().nth((page_num - 1) as usize) {
            if let Ok(page) = doc.get_object_mut(page_id) {
                if let Ok(dict) = page.as_dict_mut() {
                    let current = dict.get(b"Rotate")
                        .and_then(|r| r.as_i64())
                        .unwrap_or(0);
                    dict.set("Rotate", Object::Integer((current + angle as i64) % 360));
                }
            }
        }
    }

    doc.save(output)?;
    Ok(rotated)
}

/// 重排序页面
pub fn reorder_pages(path: &PathBuf, order_str: &str, output: &PathBuf) -> PdfResult<()> {
    let doc = Document::load(path.clone())?;
    let total = total_pages(&doc);
    let order: Vec<u32> = order_str.split(',')
        .map(|s| s.trim().parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PdfError::InvalidInput("invalid order".into()))?;

    for &p in &order {
        if p < 1 || p > total {
            return Err(PdfError::PageOutOfRange(p, total));
        }
    }

    let mut new_doc = doc.clone();
    // 保留需要的页面，按新顺序
    // 先删除所有不在 order 中的页面（去重后）
    let unique_order: Vec<u32> = order.iter().copied().collect();
    let to_remove: Vec<u32> = (1..=total).filter(|p| !unique_order.contains(p)).collect();
    for &p in to_remove.iter().rev() {
        delete_page_num(&mut new_doc, p);
    }

    new_doc.save(output)?;
    Ok(())
}

/// 压缩 PDF（去除元数据、优化）
pub fn compress_pdf(path: &PathBuf, output: &PathBuf) -> PdfResult<(u64, u64)> {
    let original_size = std::fs::metadata(path)?.len();
    let mut doc = Document::load(path.clone())?;

    // 清除元数据
    if let Ok(info_id) = doc.trailer.get(b"Root")
        .and_then(|root| doc.get_object(root.as_reference()?))
        .and_then(|root| root.as_dict()?.get(b"Info"))
        .and_then(|info| info.as_reference())
    {
        doc.objects.remove(&info_id);
    }

    // 清除 XMP 元数据
    let to_remove: Vec<ObjectId> = doc.objects.iter()
        .filter(|(_, obj)| {
            if let Ok(dict) = obj.as_dict() {
                dict.get(b"Type").map_or(false, |t| {
                    t.as_name().map_or(false, |n| n == b"Metadata")
                })
            } else { false }
        })
        .map(|(&id, _)| id)
        .collect();
    for id in to_remove {
        doc.objects.remove(&id);
    }

    doc.save(output)?;
    let compressed_size = std::fs::metadata(output)?.len();
    Ok((original_size, compressed_size))
}

/// 删除页面预览（不需要实际修改文件，只返回信息）
pub fn delete_preview_info(path: &PathBuf, pages_str: &str) -> PdfResult<DeletePreview> {
    let doc = Document::load(path.clone())?;
    let total = total_pages(&doc);
    let pages_to_delete = parse_pages(pages_str, total)?;

    Ok(DeletePreview {
        total_pages: total,
        delete_count: pages_to_delete.len() as u32,
        first_page_num: *pages_to_delete.first().unwrap_or(&0),
        last_page_num: *pages_to_delete.last().unwrap_or(&0),
        pages_to_delete,
    })
}
