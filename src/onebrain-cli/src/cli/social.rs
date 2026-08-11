//! Social features: follow, unfollow, following, peer-info, share.

use onebrain_node::node::OneBrainNode;

use super::helpers::*;

pub(crate) fn cmd_follow(node: &mut OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: follow <node_id>");
        eprintln!("    Follow a node to receive its new KUs in your feed.");
        eprintln!();
        return;
    }

    match node.follow_node(args.trim()) {
        Ok(()) => {
            println!();
            println!("  ✓ Now following node: {}", args.trim());
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_unfollow(node: &mut OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: unfollow <node_id>");
        eprintln!();
        return;
    }

    match node.unfollow_node(args.trim()) {
        Ok(()) => {
            println!();
            println!("  ✓ Unfollowed node: {}", args.trim());
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}

pub(crate) fn cmd_following(node: &OneBrainNode) {
    let list = node.following_list();

    println!();
    if list.is_empty() {
        println!("  No nodes followed yet.");
        println!("  Use 'follow <node_id>' to follow a node.");
    } else {
        println!("  ── Following ({} nodes) ──", list.len());
        println!("  {:<32}  {:<20}  Since", "Node ID", "Name");
        println!("  {}", "─".repeat(70));
        for f in &list {
            let since = format_timestamp(f.followed_since);
            let short_id = if f.node_id.len() >= 16 {
                &f.node_id[..16]
            } else {
                &f.node_id
            };
            println!("  {:<32}  {:<20}  {}", short_id, f.name, since);
        }
    }
    println!();
}

pub(crate) fn cmd_peer_info(node: &OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: peer-info <node_id>");
        eprintln!("    View public profile of another node.");
        eprintln!();
        return;
    }

    match node.get_peer_profile(args.trim()) {
        Some(profile) => {
            println!();
            println!("  ╔═══════════════════════════════════════╗");
            println!("  ║         Node Profile                  ║");
            println!("  ╚═══════════════════════════════════════╝");
            println!("  Node ID:    {}", profile.node_id);
            println!("  Name:       {}", profile.name);
            println!("  Trust:      {:.2}", profile.trust_score);
            println!("  Tier:       {}", profile.tier);
            println!("  KUs:        {}", profile.ku_count);
            if !profile.expertise.is_empty() {
                println!("  Expertise:  {}", profile.expertise.join(", "));
            }
            if profile.member_since > 0 {
                println!("  Member:     {}", format_timestamp(profile.member_since));
            }
            println!();
        }
        None => {
            eprintln!();
            eprintln!("  ✗ Node not found: {}", args.trim());
            eprintln!("    The node must be in your connected peers.");
            eprintln!();
        }
    }
}

pub(crate) fn cmd_share(node: &OneBrainNode, args: &str) {
    if args.is_empty() {
        eprintln!();
        eprintln!("  ✗ Usage: share <cid>");
        eprintln!("    Generate a shareable link for a KU.");
        eprintln!();
        return;
    }

    let cid_hex = args.trim();

    // Verify KU exists
    match node.get_ku(cid_hex) {
        Ok(detail) => {
            println!();
            println!("  ── Share KU ──");
            println!("  CID:       {}", detail.cid_hex);
            println!("  Gene type: {}", detail.gene_type);
            println!("  Preview:   {}", truncate_str(&detail.content, 60));
            println!();
            println!("  📋 Shareable link:");
            println!("  onebrain://ku/{}", detail.cid_hex);
            println!();
            println!(
                "  Recipients can use: detail {}",
                &detail.cid_hex[..std::cmp::min(16, detail.cid_hex.len())]
            );
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ {}", e);
            println!();
        }
    }
}
