use anyhow::Result;

use crate::cli::SkillAction;
use crate::client;
use crate::output::truncate_for_display;

pub(crate) async fn run(action: SkillAction) -> Result<()> {
    match action {
        SkillAction::List { status, limit } => {
            let variants = client::send_skill_list(status, limit).await?;
            if variants.is_empty() {
                println!("No skills found.");
            } else {
                println!(
                    "{:<12} {:<24} {:>5}  {:>9}  {}",
                    "STATUS", "SKILL NAME", "USES", "SUCCESS", "TAGS"
                );
                for variant in &variants {
                    let success = format!("{}/{}", variant.success_count, variant.use_count);
                    let tags = variant.context_tags.join(", ");
                    println!(
                        "{:<12} {:<24} {:>5}  {:>9}  {}",
                        variant.status, variant.skill_name, variant.use_count, success, tags
                    );
                }
                println!("\n{} skill(s) shown.", variants.len());
            }
        }
        SkillAction::Inspect { name } => {
            let (variant, content) = client::send_skill_inspect(&name).await?;
            if let Some(variant) = variant {
                println!("Skill:       {}", variant.skill_name);
                println!(
                    "Variant:     {} ({})",
                    variant.variant_name, variant.variant_id
                );
                println!("Status:      {}", variant.status);
                println!("Path:        {}", variant.relative_path);
                println!(
                    "Usage:       {} uses ({} success, {} failure)",
                    variant.use_count, variant.success_count, variant.failure_count
                );
                if !variant.context_tags.is_empty() {
                    println!("Tags:        {}", variant.context_tags.join(", "));
                }
                if let Some(content) = content {
                    println!("\n--- SKILL.md ---\n{}", content);
                }
            } else {
                eprintln!("Skill not found: {}", name);
            }
        }
        SkillAction::Reject { name } => {
            let (success, message) = client::send_skill_reject(&name).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("{}", message);
            }
        }
        SkillAction::Promote { name, to } => {
            let (success, message) = client::send_skill_promote(&name, &to).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("{}", message);
            }
        }
        SkillAction::Search { query } => {
            let entries = client::send_skill_search(&query).await?;
            if entries.is_empty() {
                println!("No community skills found for '{}'.", query);
            } else {
                println!(
                    "{:<10} {:<24} {:>6} {:>8} {:<10} {}",
                    "VERIFIED", "NAME", "USES", "SUCCESS", "PUBLISHER", "DESCRIPTION"
                );
                for entry in &entries {
                    let verified = if entry.publisher_verified { "✓" } else { "-" };
                    let success = format!("{:.0}%", entry.success_rate * 100.0);
                    let publisher = truncate_for_display(&entry.publisher_id, 8);
                    let description = truncate_for_display(&entry.description, 40);
                    println!(
                        "{:<10} {:<24} {:>6} {:>8} {:<10} {}",
                        verified,
                        truncate_for_display(&entry.name, 24),
                        entry.use_count,
                        success,
                        publisher,
                        description
                    );
                }
                println!("\n{} skill(s) found.", entries.len());
            }
        }
        SkillAction::Import { source, force } => {
            let (success, message, variant_id, scan_verdict, findings_count) =
                client::send_skill_import(&source, force).await?;
            if success {
                println!(
                    "Imported skill as Draft (variant: {}).",
                    variant_id.unwrap_or_default()
                );
                if scan_verdict.as_deref() == Some("warn") {
                    println!(
                        "Note: {} security warning(s) overridden with --force.",
                        findings_count
                    );
                }
            } else {
                match scan_verdict.as_deref() {
                    Some("block") => eprintln!("Import blocked: {}", message),
                    Some("warn") => eprintln!("Import requires --force: {}", message),
                    _ => eprintln!("{}", message),
                }
                std::process::exit(1);
            }
        }
        SkillAction::Export {
            name,
            format,
            output,
        } => {
            let (success, message, output_path) =
                client::send_skill_export(&name, &format, &output).await?;
            if success {
                println!("Exported to: {}", output_path.unwrap_or_default());
            } else {
                eprintln!("Export failed: {}", message);
                std::process::exit(1);
            }
        }
        SkillAction::Publish { name } => {
            let (success, message) = client::send_skill_publish(&name).await?;
            if success {
                println!("{}", message);
            } else {
                eprintln!("Publish failed: {}", message);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
