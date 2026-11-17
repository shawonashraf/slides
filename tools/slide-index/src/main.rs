//! Regenerates the slide list in `slides/index.html`.
//!
//! Every `slides/dd-mm-yy-name.html` file is listed, newest first. Only the
//! region between the START and END markers is rewritten; everything else in
//! index.html is left as is. If index.html does not exist, it is created from
//! a built-in template.
//!
//! Usage: slide-index [SLIDES_DIR]   (default: slides)

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const START: &str = "<!-- SLIDES:START -->";
const END: &str = "<!-- SLIDES:END -->";
const INDEX: &str = "index.html";
const TEMPLATE: &str = include_str!("template.html");
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Debug, PartialEq, Eq)]
struct Slide {
    year: u32,
    month: u32,
    day: u32,
    file: String,
    title: String,
}

fn main() -> ExitCode {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "slides".into()));
    match run(&dir) {
        Ok(n) => {
            println!("slide-index: {n} slide(s) listed in {}", dir.join(INDEX).display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("slide-index: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(dir: &Path) -> Result<usize, String> {
    let mut slides = Vec::new();
    let mut bad = Vec::new();

    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(file) = path.file_name().and_then(|f| f.to_str()) else { continue };
        if !path.is_file() || !file.ends_with(".html") || file == INDEX {
            continue;
        }
        let html = fs::read_to_string(&path).map_err(|e| format!("cannot read {file}: {e}"))?;
        match parse_slide(file, &html) {
            Some(slide) => slides.push(slide),
            None => bad.push(file.to_string()),
        }
    }

    if !bad.is_empty() {
        bad.sort();
        return Err(format!(
            "these files do not match dd-mm-yy-name.html with a valid date:\n  {}",
            bad.join("\n  ")
        ));
    }

    sort_slides(&mut slides);

    let index_path = dir.join(INDEX);
    let current = match fs::read_to_string(&index_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => TEMPLATE.to_string(),
        Err(e) => return Err(format!("cannot read {}: {e}", index_path.display())),
    };
    let updated = replace_list(&current, &render_list(&slides))?;
    if updated != current {
        fs::write(&index_path, updated)
            .map_err(|e| format!("cannot write {}: {e}", index_path.display()))?;
    }
    Ok(slides.len())
}

/// Parses `dd-mm-yy-name.html`. Returns None if the name or date is invalid.
fn parse_slide(file: &str, html: &str) -> Option<Slide> {
    let stem = file.strip_suffix(".html")?;
    let mut parts = stem.splitn(4, '-');
    let (d, m, y, name) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if [d, m, y].iter().any(|p| p.len() != 2 || !p.bytes().all(|b| b.is_ascii_digit()))
        || name.is_empty()
    {
        return None;
    }
    let (day, month, year) = (d.parse().ok()?, m.parse().ok()?, 2000 + y.parse::<u32>().ok()?);
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let title = extract_title(html).unwrap_or_else(|| name.replace(['-', '_'], " "));
    Some(Slide { year, month, day, file: file.to_string(), title })
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title>")?;
    let title = html[start..end].split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

/// Newest first; same-day slides ordered by file name.
fn sort_slides(slides: &mut [Slide]) {
    slides.sort_by(|a, b| {
        (b.year, b.month, b.day).cmp(&(a.year, a.month, a.day)).then_with(|| a.file.cmp(&b.file))
    });
}

fn render_list(slides: &[Slide]) -> String {
    let mut out = String::from("\n");
    if slides.is_empty() {
        out.push_str("      <li class=\"empty\">No slides yet.</li>\n");
    }
    for s in slides {
        let _ = writeln!(
            out,
            "      <li><a href=\"{file}\"><time datetime=\"{y:04}-{m:02}-{d:02}\">{d:02} {mon} {y}</time><span>{title}</span></a></li>",
            file = escape(&s.file),
            y = s.year,
            m = s.month,
            d = s.day,
            mon = MONTHS[s.month as usize - 1],
            title = escape(&s.title),
        );
    }
    out.push_str("      ");
    out
}

fn replace_list(html: &str, list: &str) -> Result<String, String> {
    let start = html.find(START).ok_or("index.html is missing the SLIDES:START marker")?;
    let inner = start + START.len();
    let end = inner + html[inner..].find(END).ok_or("index.html is missing the SLIDES:END marker")?;
    Ok(format!("{}{}{}", &html[..inner], list, &html[end..]))
}

/// Escapes text for HTML. Titles from <title> may already contain entities
/// such as &amp;, so only `<`, `>` and `"` are escaped, plus `&` when it does
/// not start an entity.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '&' if !starts_entity(&s[i..]) => out.push_str("&amp;"),
            _ => out.push(c),
        }
    }
    out
}

fn starts_entity(s: &str) -> bool {
    let Some(end) = s[1..].find(';') else { return false };
    let body = &s[1..=end];
    !body.is_empty()
        && body.len() <= 32
        && body.chars().all(|c| c.is_ascii_alphanumeric() || c == '#')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(file: &str) -> Slide {
        parse_slide(file, "").unwrap()
    }

    #[test]
    fn parses_valid_names() {
        let s = parse_slide("17-11-26-jax-sharding-vis.html", "<title>JAX Sharding</title>").unwrap();
        assert_eq!((s.day, s.month, s.year), (17, 11, 2026));
        assert_eq!(s.title, "JAX Sharding");
        assert_eq!(slide("01-02-25-my_talk.html").title, "my talk");
    }

    #[test]
    fn rejects_invalid_names() {
        for f in [
            "jax.html",
            "17-11-26.html",
            "17-11-26-.html",
            "1-11-26-x.html",
            "17-13-26-x.html",
            "31-04-26-x.html",
            "29-02-25-x.html",
            "00-01-26-x.html",
            "2026-11-17-x.html",
        ] {
            assert!(parse_slide(f, "").is_none(), "{f} should be rejected");
        }
        assert!(parse_slide("29-02-24-x.html", "").is_some());
    }

    #[test]
    fn sorts_newest_first_across_years() {
        let mut v = vec![
            slide("05-01-26-b.html"),
            slide("31-12-25-a.html"),
            slide("17-11-26-c.html"),
            slide("05-01-26-a.html"),
            slide("01-03-24-d.html"),
        ];
        sort_slides(&mut v);
        let files: Vec<_> = v.iter().map(|s| s.file.as_str()).collect();
        assert_eq!(
            files,
            ["17-11-26-c.html", "05-01-26-a.html", "05-01-26-b.html", "31-12-25-a.html", "01-03-24-d.html"]
        );
    }

    #[test]
    fn replaces_only_marked_region() {
        let html = format!("<p>keep</p>{START}old{END}<p>tail</p>");
        let out = replace_list(&html, "new").unwrap();
        assert_eq!(out, format!("<p>keep</p>{START}new{END}<p>tail</p>"));
        assert!(replace_list("<p>no markers</p>", "x").is_err());
    }

    #[test]
    fn escapes_but_keeps_entities() {
        assert_eq!(escape("A & B <x> &amp; &#39;"), "A &amp; B &lt;x&gt; &amp; &#39;");
    }

    #[test]
    fn template_has_markers() {
        assert!(replace_list(TEMPLATE, "x").is_ok());
    }
}
