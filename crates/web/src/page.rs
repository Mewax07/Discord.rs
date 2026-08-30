use httpd::escape_html;

use crate::manifest::{human_size, DownloadItem, Manifest};
use crate::SiteConfig;

pub fn index(config: &SiteConfig, manifest: &Manifest) -> String {
    let items = manifest.visible();

    let cards = if items.is_empty() {
        String::from("<p class=\"empty\">No download is published yet.</p>")
    } else {
        items
            .iter()
            .map(|item| card(config, item))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let discord = match &config.discord_url {
        Some(url) => format!(
            "<a class=\"ghost\" href=\"{}\" rel=\"noreferrer\">Discord</a>",
            escape_html(url)
        ),
        None => String::new(),
    };

    shell(
        config,
        &format!(
            r#"<header>
      <h1>{name}</h1>
      <p class="tagline">{tagline}</p>
      <div class="actions">{discord}</div>
    </header>
    <main>
{cards}
    </main>"#,
            name = escape_html(&config.site_name),
            tagline = escape_html(&config.tagline),
        ),
    )
}

fn card(config: &SiteConfig, item: &DownloadItem) -> String {
    let size = item
        .size(&config.files_dir)
        .map(human_size)
        .unwrap_or_else(|| "unavailable".to_string());
    let available = item.resolve(&config.files_dir).is_some();

    let meta = [
        (!item.version.is_empty()).then(|| format!("v{}", escape_html(&item.version))),
        (!item.platform.is_empty()).then(|| escape_html(&item.platform)),
        Some(size),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");

    let checksum = match &item.sha256 {
        Some(hash) if !hash.is_empty() => format!(
            "<p class=\"hash\"><span>SHA-256</span><code>{}</code></p>",
            escape_html(hash)
        ),
        _ => String::new(),
    };

    let button = if available {
        format!(
            "<a class=\"download\" href=\"/d/{id}\">Download</a>",
            id = escape_html(&item.id)
        )
    } else {
        String::from("<span class=\"download disabled\">Unavailable</span>")
    };

    format!(
        r#"      <article>
        <div class="row">
          <div>
            <h2>{name}</h2>
            <p class="meta">{meta}</p>
          </div>
          {button}
        </div>
        <p class="description">{description}</p>
        {checksum}
      </article>"#,
        name = escape_html(&item.name),
        description = escape_html(&item.description),
    )
}

pub fn not_found(config: &SiteConfig) -> String {
    shell(
        config,
        r#"<header>
      <h1>Not found</h1>
      <p class="tagline">This page or download does not exist.</p>
      <div class="actions"><a class="ghost" href="/">Back to downloads</a></div>
    </header>"#,
    )
}

fn shell(config: &SiteConfig, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
:root {{ color-scheme: dark; --accent: {accent}; --bg: #0d0d11; --surface: #16161c; --line: #26262f; --text: #ececf1; --muted: #9a9aa8; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; padding: 48px 20px 72px; background: var(--bg); color: var(--text);
  font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; line-height: 1.55; }}
header, main, footer {{ max-width: 760px; margin: 0 auto; }}
h1 {{ margin: 0; font-size: 2rem; letter-spacing: -0.02em; }}
h2 {{ margin: 0 0 4px; font-size: 1.1rem; }}
.tagline {{ margin: 8px 0 0; color: var(--muted); }}
.actions {{ margin-top: 20px; }}
main {{ margin-top: 40px; display: grid; gap: 16px; }}
article {{ background: var(--surface); border: 1px solid var(--line); border-radius: 14px; padding: 20px 22px; }}
.row {{ display: flex; align-items: center; justify-content: space-between; gap: 20px; }}
.meta {{ margin: 0; color: var(--muted); font-size: 0.875rem; }}
.description {{ margin: 14px 0 0; color: #c6c6d2; font-size: 0.95rem; }}
.hash {{ margin: 12px 0 0; font-size: 0.75rem; color: var(--muted); display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; }}
.hash code {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace; word-break: break-all; color: #b9b9c6; }}
.download {{ background: var(--accent); color: #fff; text-decoration: none; padding: 10px 20px;
  border-radius: 9px; font-weight: 600; font-size: 0.9rem; white-space: nowrap; }}
.download:hover {{ filter: brightness(1.12); }}
.download.disabled {{ background: var(--line); color: var(--muted); }}
.ghost {{ color: var(--text); text-decoration: none; border: 1px solid var(--line); padding: 9px 18px;
  border-radius: 9px; font-size: 0.9rem; }}
.ghost:hover {{ border-color: var(--accent); }}
.empty {{ color: var(--muted); }}
footer {{ margin-top: 56px; color: var(--muted); font-size: 0.8rem; text-align: center; }}
@media (max-width: 560px) {{ .row {{ flex-direction: column; align-items: flex-start; }} }}
</style>
</head>
<body>
    {body}
    <footer>{footer}</footer>
</body>
</html>
"#,
        title = escape_html(&config.site_name),
        accent = escape_html(&config.accent),
        footer = escape_html(&config.footer),
    )
}
