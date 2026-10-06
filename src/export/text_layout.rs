#[derive(Debug, PartialEq)]
pub(super) enum Kind {
    Body,
    Heading(usize),
    Metadata,
    Quote,
    List,
    Divider,
    Verbatim,
}

pub(super) struct Block {
    pub text: String,
    pub kind: Kind,
}

pub(super) fn title<'a>(title: &'a str, body: &'a str) -> &'a str {
    let title = title
        .rsplit_once("_回答")
        .filter(|(_, suffix)| {
            !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit() || c == '-')
        })
        .map_or(title, |(name, _)| name.trim_end_matches('_'))
        .trim();
    let first = body
        .trim_start_matches('\u{feff}')
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    if first.trim_end_matches(['?', '？']) == title.trim_end_matches(['?', '？']) {
        first
    } else {
        title
    }
}

fn is_answer(text: &str) -> bool {
    text.strip_prefix("回答 ")
        .and_then(|s| s.split_once('|'))
        .is_some_and(|(n, _)| n.trim().parse::<usize>().is_ok())
}

pub(super) fn metadata(text: &str) -> Option<(&str, &str)> {
    for label in ["链接", "来源", "作者"] {
        if let Some(value) = text
            .strip_prefix(label)
            .and_then(|s| s.strip_prefix([':', '：']))
        {
            return Some((label, value.trim()));
        }
    }
    None
}

fn list(text: &str) -> bool {
    text.starts_with("- ")
        || text.starts_with("* ")
        || text.starts_with("• ")
        || text.split_once(['.', ')', '、']).is_some_and(|(n, rest)| {
            !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(' ')
        })
}

pub(super) fn blocks(body: &str, title: &str) -> Vec<Block> {
    let answers = body.lines().any(|line| is_answer(line.trim()));
    let mut result: Vec<Block> = Vec::new();
    let mut pending = String::new();
    let mut fenced = false;
    let mut fence = "";
    let flush = |pending: &mut String, result: &mut Vec<Block>| {
        if !pending.is_empty() {
            result.push(Block {
                text: std::mem::take(pending),
                kind: Kind::Body,
            });
        }
    };
    let lines: Vec<_> = body.trim_start_matches('\u{feff}').lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let text = line.trim();
        if index == 0 && text.trim_end_matches(['?', '？']) == title.trim_end_matches(['?', '？'])
        {
            continue;
        }
        if fenced {
            let block = result.last_mut().unwrap();
            block.text.push('\n');
            block.text.push_str(line);
            if text.starts_with(fence) {
                fenced = false;
            }
            continue;
        }
        let marker = if text.starts_with("```") {
            "```"
        } else {
            "~~~"
        };
        let closed_fence = (text.starts_with("```") || text.starts_with("~~~"))
            && lines[index + 1..]
                .iter()
                .take_while(|line| !is_answer(line.trim()))
                .any(|line| line.trim() == marker);
        if closed_fence {
            flush(&mut pending, &mut result);
            fence = if text.starts_with("```") {
                "```"
            } else {
                "~~~"
            };
            fenced = true;
            result.push(Block {
                text: (*line).into(),
                kind: Kind::Verbatim,
            });
            continue;
        }
        if text.is_empty() {
            flush(&mut pending, &mut result);
            continue;
        }
        let hashes = text.chars().take_while(|c| *c == '#').count();
        let kind = if is_answer(text) {
            Kind::Heading(2)
        } else if hashes > 0 && hashes <= 6 && text[hashes..].starts_with(' ') {
            Kind::Heading((hashes + if answers { 2 } else { 1 }).min(6))
        } else if matches!(text, "---" | "***" | "___") {
            Kind::Divider
        } else if metadata(text).is_some()
            || text.contains("回答")
                && text.contains("导出")
                && (text.starts_with("共 ") || text.starts_with("已获取 "))
        {
            Kind::Metadata
        } else if text.starts_with('>') {
            Kind::Quote
        } else if list(text) {
            Kind::List
        } else if text.starts_with('|') || text.starts_with('[') && text.contains("]: ") {
            Kind::Verbatim
        } else {
            Kind::Body
        };
        if kind == Kind::Body {
            if pending
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii() && !c.is_whitespace())
                && text
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric())
            {
                pending.push(' ');
            }
            pending.push_str(text);
        } else {
            flush(&mut pending, &mut result);
            let text = if matches!(kind, Kind::Heading(_)) && !is_answer(text) {
                text[hashes..].trim_start()
            } else {
                text
            };
            result.push(Block {
                text: text.into(),
                kind,
            });
        }
    }
    flush(&mut pending, &mut result);
    result
}

fn escape_angles(text: &str) -> String {
    text.replace('<', "&lt;").replace('>', "&gt;")
}

pub(super) fn markdown(title_input: &str, body: &str) -> String {
    let title = title(title_input, body);
    let mut out = format!("# {}\n\n", escape_angles(title).replace('\n', " "));
    let blocks = blocks(body, title);
    for (index, block) in blocks.iter().enumerate() {
        let text = match &block.kind {
            Kind::Heading(level) => {
                format!("{} {}", "#".repeat(*level), escape_angles(&block.text))
            }
            Kind::Metadata => {
                if let Some((label, value)) = metadata(&block.text) {
                    let value = if value.starts_with("https://") || value.starts_with("http://") {
                        format!("<{value}>")
                    } else {
                        escape_angles(value)
                    };
                    format!("**{label}：** {value}")
                } else {
                    format!("> {}", escape_angles(&block.text))
                }
            }
            Kind::Quote => block.text.clone(),
            Kind::Divider => "---".into(),
            Kind::Verbatim => block.text.clone(),
            Kind::List => escape_angles(&block.text).replacen("• ", "- ", 1),
            Kind::Body => {
                let escaped = escape_angles(&block.text);
                if escaped.starts_with("~~~") {
                    escaped.replacen("~~~", "\\~\\~\\~", 1)
                } else if escaped.starts_with("```") {
                    escaped.replacen("```", "\\`\\`\\`", 1)
                } else {
                    escaped
                }
            }
        };
        out.push_str(&text);
        let tight = matches!(block.kind, Kind::List | Kind::Quote | Kind::Verbatim)
            && blocks
                .get(index + 1)
                .is_some_and(|next| next.kind == block.kind);
        out.push_str(if tight { "\n" } else { "\n\n" });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn question_has_one_title_and_navigable_answers() {
        let body = "测试问题？\n\n共 2 个回答 | 导出 1-2\n\n回答 1 | 作者\n链接：https://example.com/1\n\n中文段落\n继续末尾\n\n---\n\n回答 2 | 另一位\n\n最终段落";
        let md = markdown("测试问题__回答1-2", body);
        assert!(md.starts_with("# 测试问题？\n\n"));
        assert_eq!(md.matches("测试问题").count(), 1);
        assert!(md.contains("## 回答 1 | 作者\n\n**链接：** <https://example.com/1>"));
        assert!(md.contains("中文段落继续末尾"));
        assert!(md.contains("## 回答 2 | 另一位"));
        assert!(md.ends_with("最终段落\n\n"));
    }
    #[test]
    fn preserves_code_lists_quotes_and_source_emphasis() {
        let body = "# 正文标题\n\n**加粗** English\nwords\n\n- 第一条\n- 第二条\n\n> 引用\n> 下一行\n\n```rust\n  let x = 1;\n```\n\n<text>\n\n[1]: https://example.com";
        let md = markdown("文章", body);
        assert!(md.contains("## 正文标题"));
        assert!(md.contains("**加粗** English words"));
        assert!(md.contains("- 第一条\n- 第二条\n\n"));
        assert!(md.contains("> 引用\n> 下一行\n\n"));
        assert!(md.contains("```rust\n  let x = 1;\n```"));
        assert!(md.contains("&lt;text&gt;"));
        assert!(md.contains("[1]: https://example.com"));
    }
    #[test]
    fn conversational_tildes_do_not_swallow_later_answers() {
        let body = "回答 1 | 甲\n链接：https://example.com/1\n\n~~~\n\n后续正文\n\n回答 2 | 乙\n链接：https://example.com/2\n\n最终段落";
        let parsed = blocks(body, "问题");
        assert_eq!(
            parsed
                .iter()
                .filter(|b| matches!(b.kind, Kind::Heading(_)))
                .count(),
            2
        );
        assert_eq!(
            parsed.iter().filter(|b| b.kind == Kind::Metadata).count(),
            2
        );
        let md = markdown("问题", body);
        assert!(md.contains("\\~\\~\\~"));
        assert!(md.contains("## 回答 2 | 乙"));
    }
}
