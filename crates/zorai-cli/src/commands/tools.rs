use anyhow::Result;

use crate::cli::ToolAction;
use crate::client;
use crate::output::truncate_for_display;

pub(crate) async fn run(action: ToolAction) -> Result<()> {
    match action {
        ToolAction::List {
            limit,
            offset,
            json,
        } => {
            let result = client::send_tool_list(Some(limit), Some(offset)).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if result.items.is_empty() {
                println!("No tools available.");
            } else {
                println!("{:<28} {:<18} {}", "NAME", "REQUIRED", "DESCRIPTION");
                for item in &result.items {
                    let required = if item.required.is_empty() {
                        "-".to_string()
                    } else {
                        truncate_for_display(&item.required.join(","), 18)
                    };
                    println!(
                        "{:<28} {:<18} {}",
                        truncate_for_display(&item.name, 28),
                        required,
                        truncate_for_display(&item.description, 80),
                    );
                }
                println!(
                    "\nShowing {} tool(s) starting at offset {} of {} total.",
                    result.items.len(),
                    result.offset,
                    result.total
                );
            }
        }
        ToolAction::Search {
            query,
            limit,
            offset,
            json,
        } => {
            let result = client::send_tool_search(&query, Some(limit), Some(offset)).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if result.items.is_empty() {
                println!("No tools matched '{}'.", query);
            } else {
                println!(
                    "{:<6} {:<28} {:<18} {}",
                    "SCORE", "NAME", "MATCHED", "DESCRIPTION"
                );
                for item in &result.items {
                    let matched = if item.matched_fields.is_empty() {
                        "-".to_string()
                    } else {
                        truncate_for_display(&item.matched_fields.join(","), 18)
                    };
                    println!(
                        "{:<6} {:<28} {:<18} {}",
                        item.score,
                        truncate_for_display(&item.name, 28),
                        matched,
                        truncate_for_display(&item.description, 80),
                    );
                }
                println!(
                    "\nShowing {} match(es) starting at offset {} of {} total for '{}'.",
                    result.items.len(),
                    result.offset,
                    result.total,
                    result.query
                );
            }
        }
    }

    Ok(())
}
