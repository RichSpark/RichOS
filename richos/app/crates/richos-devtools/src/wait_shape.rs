use std::path::Path;

fn arguments(source: &str, start: usize) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut i = start;
    let mut depth = 0;
    let mut current = String::new();
    let mut out = Vec::new();
    let mut quote = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            current.push(c);
            if c == '\\' {
                i += 1;
                if i < chars.len() {
                    current.push(chars[i]);
                }
            } else if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if ['\'', '"', '`'].contains(&c) {
            quote = Some(c);
            current.push(c);
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let Some(end) = chars[i + 2..].windows(2).position(|w| w == ['*', '/']) else {
                return out;
            };
            i += end + 4;
            continue;
        }
        if "([{) ]}".replace(' ', "").contains(c) {
            if "([{ ".contains(c) {
                depth += 1;
            } else {
                if depth == 0 {
                    out.push(current);
                    return out;
                }
                depth -= 1;
            }
        }
        if c == ',' && depth == 0 {
            out.push(current);
            current = String::new();
        } else {
            current.push(c);
        }
        i += 1;
    }
    out
}

pub fn scan(root: &Path) -> Result<Vec<String>, String> {
    fn visit(
        root: &Path,
        dir: &Path,
        hits: &mut Vec<String>,
        check: &regex::Regex,
    ) -> Result<(), String> {
        let mut entries = std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if ["node_modules", ".shots"]
                .iter()
                .any(|n| entry.file_name() == *n)
            {
                continue;
            }
            let p = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                visit(root, &p, hits, check)?;
            } else if p.extension().is_some_and(|e| e == "js") {
                let source = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
                for (offset, m) in source.match_indices(".waitForFunction(") {
                    let args = arguments(&source, source[..offset + m.len()].chars().count());
                    if args.len() == 2 && check.is_match(&args[1]) {
                        hits.push(format!(
                            "{}:{}",
                            p.strip_prefix(root).unwrap().display(),
                            source[..offset].chars().filter(|c| *c == '\n').count() + 1
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    let mut hits = Vec::new();
    let check = regex::Regex::new(r"^\s*\{[^}]*\btimeout\b").unwrap();
    visit(root, root, &mut hits, &check)?;
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanner_distinguishes_argument_position() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cases.js"),"page.waitForFunction(fn, { timeout: 5 });\npage.waitForFunction(fn, null, { timeout: 5 });\npage.waitForFunction(() => ({x: 'a,b'}), { timeout: 5 });").unwrap();
        assert_eq!(scan(dir.path()).unwrap(), vec!["cases.js:1", "cases.js:3"]);
    }
    #[test]
    fn comments_templates_and_unicode_preserve_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("cases.js"),
            "const x='é';\npage.waitForFunction(() => `a,b`, /* ignored , */ { timeout: 5 });\n",
        )
        .unwrap();
        assert_eq!(scan(dir.path()).unwrap(), vec!["cases.js:2"]);
    }
}
