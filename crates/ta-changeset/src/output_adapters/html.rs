//! html.rs — HTML output adapter with JavaScript-free progressive disclosure.

use crate::error::ChangeSetError;
use crate::output_adapters::{DetailLevel, OutputAdapter, RenderContext};
use crate::pr_package::{Artifact, ChangeType};

#[derive(Default)]
pub struct HtmlAdapter {}

impl HtmlAdapter {
    pub fn new() -> Self {
        Self {}
    }

    fn change_badge(&self, change_type: &ChangeType) -> &str {
        match change_type {
            ChangeType::Add => r#"<span class="badge add">+</span>"#,
            ChangeType::Modify => r#"<span class="badge modify">~</span>"#,
            ChangeType::Delete => r#"<span class="badge delete">-</span>"#,
            ChangeType::Rename => r#"<span class="badge rename">&gt;</span>"#,
        }
    }

    fn css(&self) -> &str {
        r#"
        <style>
            body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; max-width: 1200px; margin: 0 auto; padding: 20px; line-height: 1.6; }
            h1, h2, h3 { color: #333; }
            .header { background: #f5f5f5; padding: 20px; border-radius: 8px; margin-bottom: 30px; }
            .status { display: inline-block; padding: 4px 12px; border-radius: 4px; font-weight: 600; text-transform: uppercase; font-size: 12px; }
            .status.pending { background: #fef3c7; color: #92400e; }
            .status.approved { background: #d1fae5; color: #065f46; }
            .status.denied { background: #fee2e2; color: #991b1b; }
            .artifact { background: white; border: 1px solid #e5e7eb; border-radius: 8px; padding: 20px; margin-bottom: 20px; }
            .badge { display: inline-block; width: 24px; height: 24px; text-align: center; border-radius: 4px; font-weight: 700; margin-right: 8px; }
            .badge.add { background: #d1fae5; color: #065f46; }
            .badge.modify { background: #fef3c7; color: #92400e; }
            .badge.delete { background: #fee2e2; color: #991b1b; }
            .badge.rename { background: #dbeafe; color: #1e40af; }
            details { margin-top: 15px; }
            summary { cursor: pointer; font-weight: 600; color: #4b5563; user-select: none; }
            summary:hover { color: #1f2937; }
            pre { background: #f9fafb; padding: 15px; border-radius: 4px; overflow-x: auto; }
            code { font-family: 'Monaco', 'Menlo', monospace; font-size: 13px; }
            .diff-add { color: #065f46; }
            .diff-del { color: #991b1b; }
            .meta { color: #6b7280; font-size: 14px; margin-top: 10px; }
            .tags { display: flex; gap: 8px; margin-top: 10px; }
            .tag { background: #ede9fe; color: #5b21b6; padding: 4px 12px; border-radius: 12px; font-size: 12px; }
        </style>
        "#
    }
}

impl OutputAdapter for HtmlAdapter {
    fn render(&self, ctx: &RenderContext) -> Result<String, ChangeSetError> {
        let pkg = ctx.package;
        let mut html = String::from("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"UTF-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
        html.push_str(&format!("<title>PR Package: {}</title>\n", pkg.package_id));
        html.push_str(self.css());
        html.push_str("</head>\n<body>\n");

        // Header
        html.push_str("<div class=\"header\">\n");
        html.push_str("<h1>PR Package</h1>\n");
        html.push_str(&format!("<p><strong>ID:</strong> {}</p>\n", pkg.package_id));
        html.push_str(&format!(
            "<p><strong>Status:</strong> <span class=\"status {}\">{}</span></p>\n",
            pkg.status, pkg.status
        ));
        html.push_str(&format!(
            "<p><strong>Goal:</strong> {}</p>\n",
            pkg.goal.title
        ));
        html.push_str(&format!(
            "<p><strong>Created:</strong> {}</p>\n",
            pkg.created_at.format("%Y-%m-%d %H:%M:%S")
        ));
        html.push_str("</div>\n");

        // Summary
        html.push_str("<div class=\"summary\">\n<h2>Summary</h2>\n");
        html.push_str(&format!(
            "<p><strong>What changed:</strong> {}</p>\n",
            pkg.summary.what_changed
        ));
        html.push_str(&format!(
            "<p><strong>Why:</strong> {}</p>\n",
            pkg.summary.why
        ));
        html.push_str(&format!(
            "<p><strong>Impact:</strong> {}</p>\n",
            pkg.summary.impact
        ));
        html.push_str("</div>\n");

        // Artifacts
        html.push_str(&format!(
            "<h2>Changes ({} artifacts)</h2>\n",
            pkg.changes.artifacts.len()
        ));

        let artifacts: Vec<&Artifact> = if let Some(filter) = &ctx.file_filter {
            pkg.changes
                .artifacts
                .iter()
                .filter(|a| a.resource_uri.contains(filter))
                .collect()
        } else {
            pkg.changes.artifacts.iter().collect()
        };

        for artifact in artifacts {
            html.push_str("<div class=\"artifact\">\n");
            html.push_str(&format!(
                "{} <strong>{}</strong>\n",
                self.change_badge(&artifact.change_type),
                artifact.resource_uri
            ));

            if let Some(tiers) = &artifact.explanation_tiers {
                html.push_str(&format!("<p><em>{}</em></p>\n", tiers.summary));

                if ctx.detail_level == DetailLevel::Medium || ctx.detail_level == DetailLevel::Full
                {
                    html.push_str(&format!("<p>{}</p>\n", tiers.explanation));

                    if !tiers.tags.is_empty() {
                        html.push_str("<div class=\"tags\">");
                        for tag in &tiers.tags {
                            html.push_str(&format!("<span class=\"tag\">{}</span>", tag));
                        }
                        html.push_str("</div>\n");
                    }
                }
            } else if let Some(rationale) = &artifact.rationale {
                html.push_str(&format!("<p><em>{}</em></p>\n", rationale));
            }

            if ctx.detail_level == DetailLevel::Full {
                if let Some(provider) = ctx.diff_provider {
                    if let Ok(diff) = provider.get_diff(&artifact.diff_ref) {
                        html.push_str("<details>\n<summary>View diff</summary>\n<pre><code>");
                        for line in diff.lines() {
                            if line.starts_with('+') && !line.starts_with("+++") {
                                html.push_str(&format!(
                                    "<span class=\"diff-add\">{}</span>\n",
                                    line
                                ));
                            } else if line.starts_with('-') && !line.starts_with("---") {
                                html.push_str(&format!(
                                    "<span class=\"diff-del\">{}</span>\n",
                                    line
                                ));
                            } else {
                                html.push_str(&format!("{}\n", line));
                            }
                        }
                        html.push_str("</code></pre>\n</details>\n");
                    }
                }
            }

            html.push_str("</div>\n");
        }

        html.push_str(&format!(
            "<div class=\"meta\">Generated by Trusted Autonomy v{}</div>\n",
            pkg.package_version
        ));
        html.push_str("</body>\n</html>");

        Ok(html)
    }

    fn name(&self) -> &str {
        "html"
    }
}
