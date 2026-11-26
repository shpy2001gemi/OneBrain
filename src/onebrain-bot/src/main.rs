#![forbid(unsafe_code)]
//! CLI inspector & dry-run DMZ harness for `onebrain-bot`.

use onebrain_bot::{
    build_quarantined_submission, ChatPlatform, ExternalChatSubmissionInput, BOT_QUERY_ENDPOINT,
    BOT_SUBMISSIONS_ENDPOINT,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("onebrain-bot 0.1.0 (BotPrincipal DMZ adapter; PUBLIC-only + Quarantine)");
        return;
    }
    if args.iter().any(|a| a == "--inspect-contract") {
        let sample = build_quarantined_submission(ExternalChatSubmissionInput {
            platform: ChatPlatform::Discord,
            channel_id: "chan-public".to_owned(),
            external_user_id: "ext-user-1".to_owned(),
            message_id: "msg-1".to_owned(),
            raw_text: "Water boils at 373.15 K at 1 atm".to_owned(),
            observed_at_unix_s: 1_795_710_000,
        })
        .expect("valid sample submission");
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "surface": "onebrain-bot",
                "principal": "BotPrincipal",
                "allowed_endpoints": [BOT_QUERY_ENDPOINT, BOT_SUBMISSIONS_ENDPOINT],
                "private_vault_access": false,
                "public_confirm_allowed": false,
                "somatic_p9_allowed": false,
                "sample_quarantine_submission": sample,
            }))
            .expect("json")
        );
        return;
    }
    println!(
        "onebrain-bot: restricted BotPrincipal DMZ adapter ({BOT_QUERY_ENDPOINT}, {BOT_SUBMISSIONS_ENDPOINT})"
    );
}
