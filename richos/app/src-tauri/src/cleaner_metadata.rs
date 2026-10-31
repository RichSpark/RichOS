//! A finite metadata writer dispatch. Only owned native staging files reach this module.
//! The original remains intact until the host has verified output and persisted its identity.
use richos_core::cleaner::{Entry, Projection};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn append(previous: &str, note: &str) -> String {
    if previous.is_empty() {
        note.into()
    } else {
        format!("{previous}\n\n{note}")
    }
}
fn edited(previous: &str, e: &Entry) -> Result<String> {
    if e.projection.mode == "embedded" || e.projection.mode == "older" {
        if previous != e.projection.written {
            return Err(
                "Another app changed the note inside the file. Your correction is saved in RichOS"
                    .into(),
            );
        }
        Ok(append(&e.projection.previous, &e.note))
    } else {
        Ok(append(previous, &e.note))
    }
}
fn projection(previous: String, written: String) -> Projection {
    Projection {
        mode: "embedded".into(),
        reason: String::new(),
        previous,
        written,
    }
}
fn xml_escape(text: &str) -> String {
    quick_xml::escape::escape(text).into_owned()
}
pub fn write_package(path: &Path, e: &Entry, extension: &str) -> Result<Projection> {
    let mut input = ZipArchive::new(File::open(path).map_err(err)?).map_err(err)?;
    let archive = extension == "zip";
    for i in 0..input.len() {
        let f = input.by_index(i).map_err(err)?;
        if f.encrypted()
            || !archive && f.name().map_err(err)?.starts_with("_xmlsignatures/")
            || !archive && f.name().map_err(err)?.contains("documentsignatures")
        {
            return Ok(Projection {
                reason: "The file is signed or encrypted, so its bytes were left unchanged".into(),
                ..Projection::default()
            });
        }
    }
    if !archive {
        let required = match extension {
            "docx" => "word/document.xml",
            "xlsx" => "xl/workbook.xml",
            "pptx" => "ppt/presentation.xml",
            _ => "mimetype",
        };
        if input.by_name(required).is_err() {
            return Err("The file's contents do not match its document type".into());
        }
    }

    let odf = matches!(extension, "odt" | "ods" | "odp");
    let name = if odf { "meta.xml" } else { "docProps/core.xml" };
    let mut changes = std::collections::HashMap::new();
    let (previous, written) = if archive {
        let previous = std::str::from_utf8(input.comment())
            .map_err(|_| "The existing ZIP comment has an unknown encoding")?
            .to_string();
        let written = edited(&previous, e)?;
        if written.len() > u16::MAX as usize {
            return Err("The note exceeds this ZIP file's comment capacity".into());
        }
        (previous, written)
    } else {
        let mut xml = String::new();
        match input.by_name(name) {
            Ok(mut f) => {
                if f.size() > 1024 * 1024 {
                    return Err("The document's metadata is unexpectedly large".into());
                }
                f.read_to_string(&mut xml).map_err(err)?;
            }
            Err(zip::result::ZipError::FileNotFound) => {}
            Err(error) => return Err(err(error)),
        }
        if xml.is_empty() {
            xml=if odf{r#"<?xml version="1.0" encoding="UTF-8"?><office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.3"><office:meta></office:meta></office:document-meta>"#}else{r#"<?xml version="1.0" encoding="UTF-8"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"></cp:coreProperties>"#}.into();
        }
        let (_, previous) = document_note(&xml, None, odf)?;
        let written = edited(&previous, e)?;
        let (xml, _) = document_note(&xml, Some(&written), odf)?;
        let xml = add_document_keywords(&xml, &e.labels, odf)?;
        changes.insert(name.to_string(), xml);
        if !odf && !input.file_names().filter_map(|n| n.ok()).any(|n| n == name) {
            for (part, needle, closing, addition) in [
                (
                    "[Content_Types].xml",
                    "/docProps/core.xml",
                    "</Types>",
                    r#"<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>"#,
                ),
                (
                    "_rels/.rels",
                    "metadata/core-properties",
                    "</Relationships>",
                    r#"<Relationship Id="RichCleanerCore" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>"#,
                ),
            ] {
                let mut body = String::new();
                input
                    .by_name(part)
                    .map_err(err)?
                    .read_to_string(&mut body)
                    .map_err(err)?;
                if !body.contains(needle) {
                    let position = body
                        .rfind(closing)
                        .ok_or("The document package declarations are not recognized")?;
                    body.insert_str(position, addition);
                    changes.insert(part.into(), body);
                }
            }
        }
        (previous, written)
    };
    let output = path.with_extension(format!("{extension}.rewrite"));
    let run = || -> Result<()> {
        let mut writer = ZipWriter::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)
                .map_err(err)?,
        );
        writer
            .set_raw_comment(if archive {
                written.as_bytes().into()
            } else {
                input.comment().into()
            })
            .map_err(err)?;
        if let Some(data) = input.raw_zip64_extensible_data_sector() {
            writer.set_raw_zip64_extensible_data_sector(data.into());
        }
        for i in 0..input.len() {
            let f = input.by_index(i).map_err(err)?;
            let name = f.name().map_err(err)?.to_string();
            if let Some(body) = changes.remove(&name) {
                if f.extra_data().is_some() {
                    return Err("The document metadata entry has extra fields that cannot be preserved safely".into());
                }
                let options = f
                    .options()
                    .into_full_options()
                    .with_file_comment(f.comment());
                writer.start_file(name, options).map_err(err)?;
                writer.write_all(body.as_bytes()).map_err(err)?;
            } else {
                writer.raw_copy_file(f).map_err(err)?;
            }
        }
        for (name, body) in changes {
            writer
                .start_file(name, SimpleFileOptions::default())
                .map_err(err)?;
            writer.write_all(body.as_bytes()).map_err(err)?;
        }
        let output_file = writer.finish().map_err(err)?;
        output_file.sync_all().map_err(err)?;
        drop(output_file);
        std::fs::rename(&output, path).map_err(err)?;
        Ok(())
    };
    let result = run();
    if output.exists() {
        std::fs::remove_file(&output).map_err(err)?;
    }
    result?;
    Ok(projection(previous, written))
}
pub fn write_audio(path: &Path, e: &Entry, ext: &str) -> Result<Projection> {
    use lofty::{
        config::{ParseOptions, WriteOptions},
        file::AudioFile,
        tag::TagExt,
    };
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(err)?;
    let (previous, written) = match ext {
        "mp3" => {
            use lofty::id3::v2::{CommentFrame, Frame};
            use lofty::TextEncoding;
            let audio = lofty::mpeg::MpegFile::read_from(&mut file, ParseOptions::default())
                .map_err(err)?;
            let mut tag = audio.id3v2().cloned().unwrap_or_default();
            let previous = tag
                .comments()
                .find(|c| c.language == *b"eng" && c.description.is_empty())
                .map(|c| c.content.to_string())
                .unwrap_or_default();
            let written = edited(&previous, e)?;
            tag.insert(Frame::Comment(CommentFrame::new(
                TextEncoding::UTF8,
                *b"eng",
                String::new(),
                written.clone(),
            )));
            tag.save_to_path(path, WriteOptions::default())
                .map_err(err)?;
            (previous, written)
        }
        "flac" => {
            let audio = lofty::flac::FlacFile::read_from(&mut file, ParseOptions::default())
                .map_err(err)?;
            let mut tag = audio.vorbis_comments().cloned().unwrap_or_default();
            let previous = tag.get("DESCRIPTION").unwrap_or_default().to_string();
            let written = edited(&previous, e)?;
            let remaining: Vec<_> = tag
                .get_all("DESCRIPTION")
                .skip(1)
                .map(str::to_string)
                .collect();
            drop(tag.remove("DESCRIPTION").collect::<Vec<_>>());
            tag.push("DESCRIPTION".into(), written.clone());
            for value in remaining {
                tag.push("DESCRIPTION".into(), value);
            }
            tag.save_to_path(path, WriteOptions::default())
                .map_err(err)?;
            (previous, written)
        }
        "ogg" => {
            let audio = lofty::ogg::VorbisFile::read_from(&mut file, ParseOptions::default())
                .map_err(err)?;
            let mut tag = audio.vorbis_comments().clone();
            let previous = tag.get("DESCRIPTION").unwrap_or_default().to_string();
            let written = edited(&previous, e)?;
            let remaining: Vec<_> = tag
                .get_all("DESCRIPTION")
                .skip(1)
                .map(str::to_string)
                .collect();
            drop(tag.remove("DESCRIPTION").collect::<Vec<_>>());
            tag.push("DESCRIPTION".into(), written.clone());
            for value in remaining {
                tag.push("DESCRIPTION".into(), value);
            }
            tag.save_to_path(path, WriteOptions::default())
                .map_err(err)?;
            (previous, written)
        }
        _ => return Err("Unsupported audio type".into()),
    };
    File::open(path).map_err(err)?.sync_all().map_err(err)?;
    Ok(projection(previous, written))
}
fn exif_command(resources: &Path) -> Result<std::process::Command> {
    let perl = resources.join("perl/bin/perl");
    let tool = resources.join("exiftool/exiftool");
    if !perl.is_file() || !tool.is_file() {
        return Err(
            "The bundled metadata writer is missing. Install the complete RichOS app".into(),
        );
    }
    let mut command = std::process::Command::new(perl);
    command.arg(tool).args(["-config", ""]);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"PERL") {
            command.env_remove(key);
        }
    }
    Ok(command)
}
fn read_exif(path: &Path, resources: &Path, fields: &[&str]) -> Result<serde_json::Value> {
    let mut command = exif_command(resources)?;
    command.args(["-j", "-G1", "-s", "-api", "ImageHashType=SHA256"]);
    for field in fields {
        command.arg(format!(
            "-{}",
            field.strip_suffix("-x-default").unwrap_or(field)
        ));
    }
    command.arg("--").arg(path);
    let bytes = crate::cleaner_native::tool_output(command, std::time::Duration::from_secs(30))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(err)?;
    value
        .as_array()
        .and_then(|v| v.first())
        .cloned()
        .ok_or("The metadata reader returned no file".into())
}
fn write_exif(path: &Path, e: &Entry, ext: &str, resources: &Path) -> Result<Projection> {
    let kind = read_exif(path, resources, &["FileType"])?;
    let kind = kind["File:FileType"].as_str().unwrap_or_default();
    let expected = match ext {
        "png" => matches!(kind, "PNG" | "APNG"),
        "jpg" | "jpeg" => kind == "JPEG",
        "tif" | "tiff" => kind == "TIFF",
        "webp" => matches!(kind, "WEBP" | "Extended WEBP"),
        "heic" | "heif" => matches!(kind, "HEIC" | "HEIF"),
        "avif" => kind == "AVIF",
        "psd" => kind == "PSD",
        "pdf" => kind == "PDF",
        "m4a" => matches!(kind, "M4A" | "MP4"),
        "m4v" => matches!(kind, "M4V" | "MP4"),
        "mp4" => matches!(kind, "MP4" | "M4V"),
        "mov" => kind == "MOV",
        _ => false,
    };
    if !expected {
        return Err("The file's contents do not match its filename extension".into());
    }

    let fields = if matches!(ext, "m4a" | "mp4" | "m4v" | "mov") {
        vec!["ItemList:Comment"]
    } else if ext == "pdf" {
        vec!["XMP-dc:Description-x-default", "PDF:Subject"]
    } else {
        vec!["XMP-dc:Description-x-default"]
    };
    let keyword = if matches!(ext, "m4a" | "mp4" | "m4v" | "mov") {
        "ItemList:Keyword"
    } else {
        "XMP-dc:Subject"
    };
    let mut requested = fields.clone();
    requested.push(keyword);
    if ext == "pdf" {
        requested.push("PDF:Keywords");
    }
    let current = read_exif(path, resources, &requested)?;
    let old = if e.projection.mode == "embedded" || e.projection.mode == "older" {
        Some(serde_json::from_str::<serde_json::Value>(&e.projection.written).map_err(err)?)
    } else {
        None
    };
    let prior = if old.is_some() {
        serde_json::from_str::<serde_json::Value>(&e.projection.previous).map_err(err)?
    } else {
        current.clone()
    };
    let mut written = serde_json::Map::new();
    let mut previous = serde_json::Map::new();
    let mut command = exif_command(resources)?;
    command.args(["-overwrite_original", "-P"]);
    for field in fields {
        // Read the base property's default value; write x-default explicitly so other
        // language alternatives are retained.
        let key = field.strip_suffix("-x-default").unwrap_or(field);
        let actual = current[key].as_str().unwrap_or_default();
        if old
            .as_ref()
            .is_some_and(|old| old[key].as_str().unwrap_or_default() != actual)
        {
            return Err(
                "Another app changed the embedded note. The correction is saved in RichOS".into(),
            );
        }
        let before = prior[key].as_str().unwrap_or_default();
        let value = append(before, &e.note);
        previous.insert(key.into(), serde_json::json!(before));
        written.insert(key.into(), serde_json::json!(value));
        command.arg(format!("-{field}={value}"));
    }
    // Add standard keywords without replacing the user's existing keyword values.
    let existing = match &current[keyword] {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect::<Vec<_>>(),
        serde_json::Value::String(value) => vec![value.clone()],
        _ => vec![],
    };
    if keyword == "ItemList:Keyword" {
        let mut tags = existing.clone();
        for label in &e.labels {
            if !tags.iter().any(|old| old.eq_ignore_ascii_case(label)) {
                tags.push(label.clone());
            }
        }
        if !tags.is_empty() {
            command.arg(format!("-{keyword}={}", tags.join("; ")));
        }
    } else {
        for label in &e.labels {
            if !existing.iter().any(|old| old.eq_ignore_ascii_case(label)) {
                command.arg(format!("-{keyword}+={label}"));
            }
        }
    }
    if ext == "pdf" && !e.labels.is_empty() {
        let old = current["PDF:Keywords"].as_str().unwrap_or_default();
        let mut keywords = old.to_string();
        for label in &e.labels {
            if !old
                .split([',', ';'])
                .any(|part| part.trim().eq_ignore_ascii_case(label))
            {
                if !keywords.is_empty() {
                    keywords.push_str("; ");
                }
                keywords.push_str(label);
            }
        }
        command.arg(format!("-PDF:Keywords={keywords}"));
    }
    command.arg("--").arg(path);
    crate::cleaner_native::tool_output(command, std::time::Duration::from_secs(120))?;
    let mut verify_fields: Vec<&str> = written.keys().map(String::as_str).collect();
    verify_fields.push(keyword);
    let after = read_exif(path, resources, &verify_fields)?;
    if written.iter().any(|(key, value)| after[key] != *value) {
        return Err("The embedded note did not verify. The original was left alone".into());
    }
    if !e.labels.is_empty() {
        let actual = match &after[keyword] {
            serde_json::Value::Array(v) => v.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>(),
            serde_json::Value::String(v) => vec![v.as_str()],
            _ => vec![],
        };
        if e.labels.iter().any(|label| {
            !actual.iter().any(|v| {
                v.split(';')
                    .any(|part| part.trim().eq_ignore_ascii_case(label))
            })
        }) {
            return Err("The embedded keywords did not verify".into());
        }
    }
    Ok(projection(
        serde_json::to_string(&previous).map_err(err)?,
        serde_json::to_string(&written).map_err(err)?,
    ))
}
pub fn write(path: &Path, e: &Entry, resources: &Path) -> Result<Projection> {
    let extension = e
        .path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "zip" => {
            write_package(path, e, &extension)
        }
        "mp3" | "flac" | "ogg" => write_audio(path, e, &extension),
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "webp" | "heic" | "heif" | "avif" | "psd"
        | "pdf" | "m4a" | "mp4" | "m4v" | "mov" => write_exif(path, e, &extension, resources),
        "svg" => {
            let xml = std::fs::read_to_string(path).map_err(err)?;
            let (_, previous) = svg_note(&xml, None)?;
            let written = edited(&previous, e)?;
            let (output, _) = svg_note(&xml, Some(&written))?;
            let output = add_svg_keywords(&output, &e.labels)?;
            std::fs::write(path, output).map_err(err)?;
            Ok(projection(previous, written))
        }
        _ => Ok(Projection {
            reason: "This file type has no supported in-file note field".into(),
            ..Projection::default()
        }),
    }
}

#[derive(Clone)]
struct XmlSpan {
    name: String,
    start: usize,
    content_start: usize,
    content_end: usize,
    end: usize,
    empty: bool,
    language: Option<String>,
}
fn xml_spans(xml: &str, namespace: &str, local: &str) -> Result<Vec<XmlSpan>> {
    use quick_xml::{events::Event, name::ResolveResult, NsReader};
    let mut reader = NsReader::from_str(xml);
    let mut stack: Vec<Option<XmlSpan>> = vec![];
    let mut result = vec![];
    loop {
        let before = reader.buffer_position() as usize;
        let (resolved, event) = reader.read_resolved_event().map_err(err)?;
        let namespace_matches =
            matches!(resolved,ResolveResult::Bound(ref ns) if ns.as_ref()==namespace.as_bytes());
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = xml[..reader.buffer_position() as usize].ends_with("/>");
                let matches = namespace_matches && e.local_name().as_ref() == local.as_bytes();
                let value = if matches {
                    let language = e
                        .attributes()
                        .with_checks(true)
                        .find_map(|attribute| match attribute {
                            Ok(a) if a.key.as_ref() == b"xml:lang" => Some(
                                a.normalized_value(quick_xml::XmlVersion::default())
                                    .map(|v| v.into_owned())
                                    .map_err(err),
                            ),
                            _ => None,
                        })
                        .transpose()?;
                    Some(XmlSpan {
                        name: String::from_utf8(e.name().as_ref().to_vec()).map_err(err)?,
                        start: before,
                        content_start: reader.buffer_position() as usize,
                        content_end: reader.buffer_position() as usize,
                        end: reader.buffer_position() as usize,
                        empty,
                        language,
                    })
                } else {
                    None
                };
                if empty {
                    if let Some(value) = value {
                        result.push(value)
                    }
                } else {
                    stack.push(value)
                }
            }
            Event::End(_) => {
                if let Some(Some(mut span)) = stack.pop() {
                    span.content_end = before;
                    span.end = reader.buffer_position() as usize;
                    result.push(span)
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    result.sort_by_key(|s| s.start);
    Ok(result)
}
fn xml_text(xml: &str) -> Result<String> {
    use quick_xml::{events::Event, Reader};
    let mut reader = Reader::from_str(xml);
    let mut text = String::new();
    loop {
        match reader.read_event().map_err(err)? {
            Event::Text(value) => text.push_str(&value.decode().map_err(err)?),
            Event::GeneralRef(value) => {
                let reference = format!("&{};", value.decode().map_err(err)?);
                text.push_str(&quick_xml::escape::unescape(&reference).map_err(err)?);
            }
            Event::CData(value) => text.push_str(&value.decode().map_err(err)?),
            Event::Eof => return Ok(text),
            Event::Comment(_) => {}
            _ => {
                return Err(
                    "This description contains structured metadata that cannot be replaced safely"
                        .into(),
                )
            }
        }
    }
}
fn replace_content(xml: &str, span: &XmlSpan, content: &str) -> String {
    if span.empty {
        let opening = &xml[span.start..span.end - 2];
        format!(
            "{}{opening}>{content}</{}>{}",
            &xml[..span.start],
            span.name,
            &xml[span.end..]
        )
    } else {
        format!(
            "{}{}{}",
            &xml[..span.content_start],
            content,
            &xml[span.content_end..]
        )
    }
}
fn document_note(xml: &str, new: Option<&str>, odf: bool) -> Result<(String, String)> {
    let descriptions = xml_spans(xml, "http://purl.org/dc/elements/1.1/", "description")?;
    if descriptions.len() > 1 {
        return Err("Multiple existing descriptions are left unchanged".into());
    }
    let previous = descriptions
        .first()
        .map(|span| {
            if span.empty {
                Ok(String::new())
            } else {
                xml_text(&xml[span.content_start..span.content_end])
            }
        })
        .transpose()?
        .unwrap_or_default();
    let Some(note) = new else {
        return Ok((xml.into(), previous));
    };
    if let Some(span) = descriptions.first() {
        return Ok((replace_content(xml, span, &xml_escape(note)), previous));
    }
    let (namespace, local) = if odf {
        ("urn:oasis:names:tc:opendocument:xmlns:office:1.0", "meta")
    } else {
        (
            "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
            "coreProperties",
        )
    };
    let containers = xml_spans(xml, namespace, local)?;
    let span = containers
        .first()
        .ok_or("The document metadata container is unavailable")?;
    let body = if span.empty {
        ""
    } else {
        &xml[span.content_start..span.content_end]
    };
    Ok((replace_content(xml,span,&format!("{body}<dc:description xmlns:dc=\"http://purl.org/dc/elements/1.1/\">{}</dc:description>",xml_escape(note))),previous))
}
fn svg_note(xml: &str, new: Option<&str>) -> Result<(String, String)> {
    let descriptions = xml_spans(xml, "http://purl.org/dc/elements/1.1/", "description")?;
    if descriptions.len() > 1 {
        return Err("Multiple XMP descriptions are left unchanged".into());
    }
    if let Some(description) = descriptions.first() {
        let alternatives = xml_spans(xml, "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "li")?;
        let default = alternatives.iter().find(|s| {
            s.start > description.start
                && s.end < description.end
                && s.language.as_deref() == Some("x-default")
        });
        let previous = if let Some(span) = default {
            xml_text(&xml[span.content_start..span.content_end])?
        } else if !xml[description.content_start..description.content_end].contains('<') {
            xml_text(&xml[description.content_start..description.content_end])?
        } else {
            String::new()
        };
        let Some(note) = new else {
            return Ok((xml.into(), previous));
        };
        if let Some(span) = default {
            return Ok((replace_content(xml, span, &xml_escape(note)), previous));
        }
        let alts = xml_spans(xml, "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "Alt")?;
        let item=format!("<rdf:li xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xml:lang=\"x-default\">{}</rdf:li>",xml_escape(note));
        if let Some(span) = alts
            .iter()
            .find(|s| s.start > description.start && s.end < description.end)
        {
            let body = if span.empty {
                ""
            } else {
                &xml[span.content_start..span.content_end]
            };
            return Ok((
                replace_content(xml, span, &format!("{body}{item}")),
                previous,
            ));
        }
        return Ok((replace_content(xml,description,&format!("<rdf:Alt xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">{item}</rdf:Alt>")),previous));
    }
    let Some(note) = new else {
        return Ok((xml.into(), String::new()));
    };
    let field=format!("<dc:description xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><rdf:Alt xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>",xml_escape(note));
    let rdf = xml_spans(
        xml,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        "Description",
    )?;
    if let Some(span) = rdf.first() {
        let body = if span.empty {
            ""
        } else {
            &xml[span.content_start..span.content_end]
        };
        return Ok((
            replace_content(xml, span, &format!("{body}{field}")),
            String::new(),
        ));
    }
    let roots = xml_spans(xml, "http://www.w3.org/2000/svg", "svg")?;
    let svg = roots.first().ok_or("The SVG namespace is unavailable")?;
    let metadata=format!("<metadata xmlns=\"http://www.w3.org/2000/svg\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\">{field}</rdf:Description></rdf:RDF></metadata>");
    let body = if svg.empty {
        ""
    } else {
        &xml[svg.content_start..svg.content_end]
    };
    Ok((
        replace_content(xml, svg, &format!("{metadata}{body}")),
        String::new(),
    ))
}

fn equal_stream(mut a: impl Read, mut b: impl Read) -> Result<()> {
    let mut x = [0u8; 65536];
    let mut y = [0u8; 65536];
    loop {
        let nx = a.read(&mut x).map_err(err)?;
        let ny = b.read(&mut y).map_err(err)?;
        if nx != ny || x[..nx] != y[..ny] {
            return Err(
                "The staged file changed original content. The original was not replaced".into(),
            );
        }
        if nx == 0 {
            return Ok(());
        }
    }
}
fn skip_id3(file: &mut File) -> Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut header = [0u8; 10];
    let read = file.read(&mut header).map_err(err)?;
    if read == 10 && &header[..3] == b"ID3" {
        if header[6..10].iter().any(|v| v & 0x80 != 0) {
            return Err("Invalid ID3 size".into());
        }
        let length = header[6..10]
            .iter()
            .fold(0u64, |size, b| (size << 7) | *b as u64);
        let footer = if header[3] == 4 && header[5] & 0x10 != 0 {
            10
        } else {
            0
        };
        file.seek(SeekFrom::Start(10 + length + footer))
            .map_err(err)?;
    } else {
        file.seek(SeekFrom::Start(0)).map_err(err)?;
    }
    Ok(())
}
fn flac_payload(file: &mut File) -> Result<Vec<(u8, Vec<u8>)>> {
    let mut magic = [0; 4];
    file.read_exact(&mut magic).map_err(err)?;
    if &magic != b"fLaC" {
        return Err("The file is not FLAC".into());
    }
    let mut blocks = vec![];
    let mut bytes = 0usize;
    loop {
        let mut header = [0u8; 4];
        file.read_exact(&mut header).map_err(err)?;
        let length =
            ((header[1] as usize) << 16) | ((header[2] as usize) << 8) | header[3] as usize;
        bytes += length;
        if bytes > 64 * 1024 * 1024 {
            return Err("FLAC metadata exceeds the verified limit".into());
        }
        let mut body = vec![0; length];
        file.read_exact(&mut body).map_err(err)?;
        let kind = header[0] & 0x7f;
        if !matches!(kind, 1 | 4) {
            blocks.push((kind, body));
        }
        if header[0] & 0x80 != 0 {
            return Ok(blocks);
        }
    }
}
fn psd_payload(file: &mut File) -> Result<(Vec<u8>, Vec<Vec<u8>>)> {
    let mut header = vec![0u8; 26];
    file.read_exact(&mut header).map_err(err)?;
    if &header[..4] != b"8BPS" {
        return Err("The file is not Photoshop".into());
    }
    fn area(file: &mut File) -> Result<Vec<u8>> {
        let mut size = [0u8; 4];
        file.read_exact(&mut size).map_err(err)?;
        let size = u32::from_be_bytes(size) as usize;
        if size > 64 * 1024 * 1024 {
            return Err("Photoshop metadata exceeds the verified limit".into());
        }
        let mut area = vec![0; size];
        file.read_exact(&mut area).map_err(err)?;
        Ok(area)
    }
    header.extend(area(file)?);
    let data = area(file)?;
    let mut input = std::io::Cursor::new(data);
    let mut resources = vec![];
    while input.position() < (input.get_ref().len() as u64) {
        let start = input.position() as usize;
        let mut fixed = [0; 7];
        input.read_exact(&mut fixed).map_err(err)?;
        let id = u16::from_be_bytes([fixed[4], fixed[5]]);
        let name = (fixed[6] as usize + 1 + 1) & !1;
        let mut rest = vec![0; name - 1];
        input.read_exact(&mut rest).map_err(err)?;
        let mut size = [0u8; 4];
        input.read_exact(&mut size).map_err(err)?;
        let size = u32::from_be_bytes(size) as usize;
        if size > 64 * 1024 * 1024 {
            return Err("Photoshop resource exceeds the verified limit".into());
        }
        let mut body = vec![0; size + (size & 1)];
        input.read_exact(&mut body).map_err(err)?;
        if id != 1060 {
            resources.push(input.get_ref()[start..input.position() as usize].to_vec());
        }
    }
    Ok((header, resources))
}
pub fn verify_content(source: &Path, staged: &Path, ext: &str, resources: &Path) -> Result<()> {
    match ext {
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "zip" => {
            let mut original = ZipArchive::new(File::open(source).map_err(err)?).map_err(err)?;
            let mut changed = ZipArchive::new(File::open(staged).map_err(err)?).map_err(err)?;
            for index in 0..original.len() {
                let before = original.by_index_raw(index).map_err(err)?;
                let name = before.name().map_err(err)?.to_string();
                if ext != "zip"
                    && matches!(
                        name.as_str(),
                        "docProps/core.xml" | "meta.xml" | "[Content_Types].xml" | "_rels/.rels"
                    )
                {
                    continue;
                }
                let index = changed
                    .index_for_name_raw(before.name_raw())
                    .ok_or("An archive entry was removed")?;
                let after = changed.by_index_raw(index).map_err(err)?;
                if before.comment() != after.comment()
                    || before.compression() != after.compression()
                    || before.crc32() != after.crc32()
                    || before.external_attributes() != after.external_attributes()
                {
                    return Err("An unrelated archive entry changed".into());
                }
                // Read compressed bytes; no extraction, decompression bomb or codec runs.
                equal_stream(before, after)?;
            }
            Ok(())
        }
        "svg" => {
            let a = std::fs::read_to_string(source).map_err(err)?;
            let b = std::fs::read_to_string(staged).map_err(err)?;
            // Annotation changes only the existing metadata section or inserts one. Compare
            // every rendering byte outside metadata, with namespace-aware section boundaries.
            fn rendering(xml: &str) -> Result<String> {
                let spans = xml_spans(xml, "http://www.w3.org/2000/svg", "metadata")?;
                let mut out = String::new();
                let mut end = 0;
                for span in spans {
                    if span.start < end {
                        return Err("Nested SVG metadata is left unchanged".into());
                    }
                    out.push_str(&xml[end..span.start]);
                    end = span.end;
                }
                out.push_str(&xml[end..]);
                Ok(out)
            }
            if rendering(&a)? != rendering(&b)? {
                return Err("The SVG rendering content changed".into());
            }
            Ok(())
        }
        "pdf" => {
            let mut original = File::open(source).map_err(err)?;
            let mut changed = File::open(staged).map_err(err)?;
            let a = pdf_base_length(&mut original)?;
            let b = pdf_base_length(&mut changed)?;
            if a != b {
                return Err("The PDF document prefix changed".into());
            }
            equal_stream(original.take(a), changed.take(b))
        }
        "psd" => {
            let mut a = File::open(source).map_err(err)?;
            let mut b = File::open(staged).map_err(err)?;
            if psd_payload(&mut a)? != psd_payload(&mut b)? {
                return Err("Unrelated Photoshop resources changed".into());
            }
            equal_stream(a, b)
        }
        "mp3" => {
            let mut a = File::open(source).map_err(err)?;
            let mut b = File::open(staged).map_err(err)?;
            skip_id3(&mut a)?;
            skip_id3(&mut b)?;
            equal_stream(a, b)
        }
        "flac" => {
            let mut a = File::open(source).map_err(err)?;
            let mut b = File::open(staged).map_err(err)?;
            if flac_payload(&mut a)? != flac_payload(&mut b)? {
                return Err("Unrelated FLAC metadata changed".into());
            }
            equal_stream(a, b)
        }
        "ogg" => verify_vorbis(source, staged),
        _ => {
            let fields = ["FileType", "ImageDataHash"];
            let a = read_exif(source, resources, &fields)?;
            let b = read_exif(staged, resources, &fields)?;
            let hash = a["File:ImageDataHash"]
                .as_str()
                .ok_or("The writer cannot verify this file's media payload")?;
            if hash != b["File:ImageDataHash"].as_str().unwrap_or_default()
                || (a["File:FileType"] != b["File:FileType"]
                    && !(ext == "webp"
                        && matches!(a["File:FileType"].as_str(), Some("WEBP" | "Extended WEBP"))
                        && matches!(b["File:FileType"].as_str(), Some("WEBP" | "Extended WEBP"))))
            {
                return Err("The staged image or media payload changed".into());
            }
            Ok(())
        }
    }
}
fn verify_vorbis(source: &Path, staged: &Path) -> Result<()> {
    fn next(reader: &mut ogg::reading::PacketReader<File>) -> Result<Option<ogg::Packet>> {
        loop {
            match reader.read_packet().map_err(err)? {
                Some(packet) if packet.data.starts_with(b"\x03vorbis") => {}
                other => return Ok(other),
            }
        }
    }
    let mut a = ogg::reading::PacketReader::new(File::open(source).map_err(err)?);
    let mut b = ogg::reading::PacketReader::new(File::open(staged).map_err(err)?);
    loop {
        match (next(&mut a)?, next(&mut b)?) {
            (None, None) => return Ok(()),
            (Some(a), Some(b))
                if a.data == b.data
                    && a.stream_serial() == b.stream_serial()
                    && a.last_in_stream() == b.last_in_stream()
                    && (!a.last_in_stream() || a.absgp_page() == b.absgp_page()) => {}
            _ => return Err("The Vorbis encoded packets or streams changed".into()),
        }
    }
}
fn add_document_keywords(xml: &str, labels: &[String], odf: bool) -> Result<String> {
    if labels.is_empty() {
        return Ok(xml.into());
    }
    let (namespace, local, container_ns, container_local, qualified) = if odf {
        (
            "urn:oasis:names:tc:opendocument:xmlns:meta:1.0",
            "keyword",
            "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
            "meta",
            "meta:keyword",
        )
    } else {
        (
            "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
            "keywords",
            "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
            "coreProperties",
            "cp:keywords",
        )
    };
    let fields = xml_spans(xml, namespace, local)?;
    let mut existing = vec![];
    for span in &fields {
        existing.push(xml_text(&xml[span.content_start..span.content_end])?);
    }
    let added: Vec<_> = labels
        .iter()
        .filter(|label| {
            !existing.iter().any(|value| {
                value
                    .split([',', ';'])
                    .any(|part| part.trim().eq_ignore_ascii_case(label))
            })
        })
        .collect();
    if added.is_empty() {
        return Ok(xml.into());
    }
    if !odf && !fields.is_empty() {
        if fields.len() != 1 {
            return Err("Multiple native keyword fields cannot be replaced safely".into());
        }
        let current = &existing[0];
        let tags = added
            .into_iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("; ");
        return Ok(replace_content(
            xml,
            &fields[0],
            &xml_escape(&if current.is_empty() {
                tags
            } else {
                format!("{current}; {tags}")
            }),
        ));
    }
    let containers = xml_spans(xml, container_ns, container_local)?;
    let span = containers
        .first()
        .ok_or("Missing document metadata container")?;
    let body = if span.empty {
        ""
    } else {
        &xml[span.content_start..span.content_end]
    };
    let tags = if odf {
        added
            .iter()
            .map(|tag| {
                format!(
                    "<{qualified} xmlns:meta=\"{namespace}\">{}</{qualified}>",
                    xml_escape(tag)
                )
            })
            .collect::<String>()
    } else {
        format!(
            "<{qualified} xmlns:cp=\"{namespace}\">{}</{qualified}>",
            xml_escape(
                &added
                    .into_iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        )
    };
    Ok(replace_content(xml, span, &format!("{body}{tags}")))
}
fn add_svg_keywords(xml: &str, labels: &[String]) -> Result<String> {
    if labels.is_empty() {
        return Ok(xml.into());
    }
    let subjects = xml_spans(xml, "http://purl.org/dc/elements/1.1/", "subject")?;
    let bags = xml_spans(xml, "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "Bag")?;
    let values = xml_spans(xml, "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "li")?;
    let existing: Vec<_> = values
        .iter()
        .filter(|li| {
            subjects
                .iter()
                .any(|s| li.start > s.start && li.end < s.end)
        })
        .map(|span| xml_text(&xml[span.content_start..span.content_end]))
        .collect::<Result<_>>()?;
    let tags = labels
        .iter()
        .filter(|tag| !existing.iter().any(|v| v.eq_ignore_ascii_case(tag)))
        .map(|tag| {
            format!(
                "<rdf:li xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">{}</rdf:li>",
                xml_escape(tag)
            )
        })
        .collect::<String>();
    if tags.is_empty() {
        return Ok(xml.into());
    }
    if let Some(subject) = subjects.first() {
        let bag = bags
            .iter()
            .find(|b| b.start > subject.start && b.end < subject.end)
            .ok_or("The existing XMP keyword structure cannot be changed safely")?;
        let body = if bag.empty {
            ""
        } else {
            &xml[bag.content_start..bag.content_end]
        };
        return Ok(replace_content(xml, bag, &format!("{body}{tags}")));
    }
    let descriptions = xml_spans(
        xml,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        "Description",
    )?;
    let span = descriptions.first().ok_or("Missing XMP description")?;
    let body = if span.empty {
        ""
    } else {
        &xml[span.content_start..span.content_end]
    };
    Ok(replace_content(xml,span,&format!("{body}<dc:subject xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><rdf:Bag xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">{tags}</rdf:Bag></dc:subject>")))
}

/// Reopen rewritten containers before allowing the native original replacement.
pub fn verify_note(path: &Path, projection: &Projection, ext: &str) -> Result<()> {
    use lofty::{config::ParseOptions, file::AudioFile};
    let actual = match ext {
        "zip" => {
            let file = ZipArchive::new(File::open(path).map_err(err)?).map_err(err)?;
            std::str::from_utf8(file.comment())
                .map_err(err)?
                .to_string()
        }
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" => {
            let odf = matches!(ext, "odt" | "ods" | "odp");
            let mut archive = ZipArchive::new(File::open(path).map_err(err)?).map_err(err)?;
            let mut xml = String::new();
            archive
                .by_name(if odf { "meta.xml" } else { "docProps/core.xml" })
                .map_err(err)?
                .take(1024 * 1024 + 1)
                .read_to_string(&mut xml)
                .map_err(err)?;
            if xml.len() > 1024 * 1024 {
                return Err("Rewritten document metadata exceeds its limit".into());
            }
            document_note(&xml, None, odf)?.1
        }
        "svg" => svg_note(&std::fs::read_to_string(path).map_err(err)?, None)?.1,
        "mp3" => {
            let audio = lofty::mpeg::MpegFile::read_from(
                &mut File::open(path).map_err(err)?,
                ParseOptions::default(),
            )
            .map_err(err)?;
            audio
                .id3v2()
                .and_then(|tag| {
                    tag.comments()
                        .find(|c| c.language == *b"eng" && c.description.is_empty())
                })
                .map(|c| c.content.to_string())
                .unwrap_or_default()
        }
        "flac" => {
            let audio = lofty::flac::FlacFile::read_from(
                &mut File::open(path).map_err(err)?,
                ParseOptions::default(),
            )
            .map_err(err)?;
            audio
                .vorbis_comments()
                .and_then(|tag| tag.get("DESCRIPTION"))
                .unwrap_or_default()
                .to_string()
        }
        "ogg" => {
            let audio = lofty::ogg::VorbisFile::read_from(
                &mut File::open(path).map_err(err)?,
                ParseOptions::default(),
            )
            .map_err(err)?;
            audio
                .vorbis_comments()
                .get("DESCRIPTION")
                .unwrap_or_default()
                .to_string()
        }
        // ExifTool's fresh reader has already verified these fields in write_exif.
        _ => return Ok(()),
    };
    if actual != projection.written {
        return Err("The newly embedded note did not verify".into());
    }
    Ok(())
}

// ExifTool replaces only its previous incremental metadata revision on edit. All
// document bytes before that revision must remain byte-identical. This reads the
// writer's documented markers and never edits PDF objects or xref tables.
fn pdf_base_length(file: &mut File) -> Result<u64> {
    use std::io::{Seek, SeekFrom};
    let size = file.metadata().map_err(err)?.len();
    let mut header = [0; 5];
    file.read_exact(&mut header).map_err(err)?;
    if &header != b"%PDF-" {
        return Err("This PDF has an unsupported leading wrapper".into());
    }
    let n = size.min(64);
    file.seek(SeekFrom::End(-(n as i64))).map_err(err)?;
    let mut tail = vec![0; n as usize];
    file.read_exact(&mut tail).map_err(err)?;
    let text = String::from_utf8_lossy(&tail);
    let marker = "%EndExifToolUpdate ";
    let length = if let Some(start) = text.rfind(marker) {
        let number = text[start + marker.len()..]
            .split_whitespace()
            .next()
            .ok_or("Missing PDF update offset")?;
        let offset = number.parse::<u64>().map_err(err)?;
        if offset >= size {
            return Err("Invalid PDF update offset".into());
        }
        file.seek(SeekFrom::Start(offset)).map_err(err)?;
        let mut begin = [0; 20];
        file.read_exact(&mut begin).map_err(err)?;
        if &begin != b"%BeginExifToolUpdate" {
            return Err("The previous PDF metadata update is corrupt".into());
        }
        offset
    } else {
        size
    };
    file.seek(SeekFrom::Start(0)).map_err(err)?;
    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xml_note_keeps_entities_and_unrelated_properties() {
        let xml = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:creator>Original author</dc:creator><dc:description>O&apos;Brien &amp; café 📷</dc:description></cp:coreProperties>"#;
        assert_eq!(
            document_note(xml, None, false).unwrap().1,
            "O'Brien & café 📷"
        );
        let updated = document_note(xml, Some("Corrected <estimate> & context"), false)
            .unwrap()
            .0;
        assert!(updated.contains("<dc:creator>Original author</dc:creator>"));
        assert_eq!(
            document_note(&updated, None, false).unwrap().1,
            "Corrected <estimate> & context"
        );
        let keywords = add_document_keywords(&updated, &["renovation".into()], false).unwrap();
        assert!(keywords.contains("renovation"));
        assert!(keywords.contains("Original author"));
    }
    #[test]
    fn svg_keeps_other_language_alternatives_and_rendering() {
        let xml = r#"<svg xmlns="http://www.w3.org/2000/svg"><metadata><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description><dc:description xmlns:dc="http://purl.org/dc/elements/1.1/"><rdf:Alt><rdf:li xml:lang="fr">Contexte français</rdf:li></rdf:Alt></dc:description></rdf:Description></rdf:RDF></metadata><rect width="32" height="32"/></svg>"#;
        let updated = svg_note(xml, Some("A new context & reminder 📷"))
            .unwrap()
            .0;
        assert!(updated.contains("<rdf:li xml:lang=\"fr\">Contexte français</rdf:li>"));
        assert!(updated.contains("<rect width=\"32\" height=\"32\"/>"));
        assert_eq!(
            svg_note(&updated, None).unwrap().1,
            "A new context & reminder 📷"
        );
        let keywords = add_svg_keywords(&updated, &["renovation".into()]).unwrap();
        assert!(keywords.contains("renovation"));
        assert_eq!(
            svg_note(&keywords, None).unwrap().1,
            "A new context & reminder 📷"
        );
    }
    #[test]
    fn conflicting_notes_are_never_overwritten() {
        let mut e = Entry::new(
            "/Desktop/a.zip".into(),
            richos_core::cleaner::Identity {
                volume: 1,
                object: 2,
                size: 1,
                modified: "1".into(),
                changed: "1".into(),
                bookmark: String::new(),
            },
            "Corrected note".into(),
        )
        .unwrap();
        e.projection = projection(
            "Existing user comment".into(),
            "Earlier owned projection".into(),
        );
        assert!(edited("Another app's replacement", &e).is_err());
        assert_eq!(
            edited("Earlier owned projection", &e).unwrap(),
            "Existing user comment\n\nCorrected note"
        );
    }
}
