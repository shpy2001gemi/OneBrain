//! OBT wallet commands.

use onebrain_node::node::OneBrainNode;

use super::helpers::*;

pub(crate) fn cmd_wallet(node: &OneBrainNode, args: &str) {
    let subcmd = args.trim();

    if subcmd.is_empty() {
        // Show balance
        match node.get_balance() {
            Ok(wallet) => {
                println!();
                println!("  ── OBT Wallet Simulation (NON-ECONOMIC) ──");
                println!("  ⚠ No AccountChain, settlement, or spendable OBT is connected.");
                println!("  Simulated balance: {}", format_obt_short(wallet.balance));
                println!("  Simulated chain:   {} records", wallet.chain_length);
                for limitation in &wallet.limitations {
                    println!("  ⚠ {}", limitation);
                }
                println!();
                println!("  ── Tier ──");
                println!(
                    "  Current:     {} (multiplier: {:.2}x)",
                    wallet.tier, wallet.multiplier
                );
                println!();
                println!("  ── Simulated Activity Projection ──");
                println!(
                    "  Derived credit: {}",
                    format_obt_short(wallet.total_earned)
                );
                println!("  Derived debit:  {}", format_obt_short(wallet.total_spent));
                println!();

                let max_stream = [
                    wallet.streams.owner,
                    wallet.streams.encoder,
                    wallet.streams.verifier,
                    wallet.streams.storage,
                ]
                .into_iter()
                .max()
                .unwrap_or(1);

                println!("  By stream:");
                println!(
                    "    R1 Owner (40%):    {:<16} {}",
                    format_obt_short(wallet.streams.owner),
                    bar_chart(wallet.streams.owner, max_stream, 16)
                );
                println!(
                    "    R2 Encoder (25%):  {:<16} {}",
                    format_obt_short(wallet.streams.encoder),
                    bar_chart(wallet.streams.encoder, max_stream, 16)
                );
                println!(
                    "    R3 Verifier (15%): {:<16} {}",
                    format_obt_short(wallet.streams.verifier),
                    bar_chart(wallet.streams.verifier, max_stream, 16)
                );
                println!(
                    "    R4 Storage (20%):  {:<16} {}",
                    format_obt_short(wallet.streams.storage),
                    bar_chart(wallet.streams.storage, max_stream, 16)
                );
                println!();
                println!("  ── Rate Limits ──");
                println!("  KU/hour:     {} ({} tier)", wallet.rate_max, wallet.tier);
                println!(
                    "  Used:        {}/{} this hour",
                    wallet.rate_used, wallet.rate_max
                );
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else if subcmd.starts_with("history") {
        // Parse optional --limit N
        let mut limit: usize = 10;
        let parts: Vec<&str> = subcmd.split_whitespace().collect();
        let mut i = 1;
        while i < parts.len() {
            if parts[i] == "--limit" && i + 1 < parts.len() {
                limit = parts[i + 1].parse().unwrap_or(10);
                i += 2;
            } else {
                i += 1;
            }
        }

        match node.get_wallet_history(limit) {
            Ok(transactions) => {
                println!();
                if transactions.is_empty() {
                    println!("  ── Simulated History (NON-ECONOMIC) ──");
                    println!("  No transactions yet.");
                } else {
                    println!(
                        "  ── Simulated History (latest {}, NON-ECONOMIC) ──",
                        transactions.len()
                    );
                    println!(
                        "  {:>3}  {:<8} {:>14}  {:<12}  Detail",
                        "#", "Type", "Amount", "When"
                    );
                    for (i, tx) in transactions.iter().enumerate() {
                        println!(
                            "  {:>3}. {:<8} {:>14}  {:<12}  {}",
                            i + 1,
                            tx.block_type,
                            format_obt_signed(tx.amount),
                            format_timestamp(tx.timestamp),
                            tx.detail
                        );
                    }
                }
                println!();
            }
            Err(e) => {
                eprintln!("  ✗ {}", e);
                println!();
            }
        }
    } else {
        eprintln!();
        eprintln!(
            "  ✗ Unknown wallet subcommand '{}'. Use: wallet, wallet history",
            subcmd
        );
        eprintln!();
    }
}
