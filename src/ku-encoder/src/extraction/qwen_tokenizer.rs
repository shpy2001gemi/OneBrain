//! Read tokenizer metadata from the verified, locked GGUF; no remote tokenizer.
use super::*;
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom};

fn read<const N: usize>(r: &mut impl Read) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    r.read_exact(&mut bytes)
        .map_err(|_| ExtractionError("gguf_metadata"))?;
    Ok(bytes)
}
fn string(r: &mut impl Read) -> Result<String> {
    let n = u64::from_le_bytes(read(r)?);
    require(n <= 1_048_576, "gguf_metadata")?;
    let mut bytes = vec![0; n as usize];
    r.read_exact(&mut bytes)
        .map_err(|_| ExtractionError("gguf_metadata"))?;
    String::from_utf8(bytes).map_err(|_| ExtractionError("gguf_metadata"))
}
fn value(r: &mut impl Read, ty: u32, nested: bool) -> Result<Value> {
    Ok(match ty {
        0 => json!(u8::from_le_bytes(read(r)?)),
        1 => json!(i8::from_le_bytes(read(r)?)),
        2 => json!(u16::from_le_bytes(read(r)?)),
        3 => json!(i16::from_le_bytes(read(r)?)),
        4 => json!(u32::from_le_bytes(read(r)?)),
        5 => json!(i32::from_le_bytes(read(r)?)),
        6 => json!(f32::from_le_bytes(read(r)?)),
        7 => json!(u8::from_le_bytes(read(r)?) != 0),
        8 => json!(string(r)?),
        9 if !nested => {
            let subtype = u32::from_le_bytes(read(r)?);
            let n = u64::from_le_bytes(read(r)?);
            require(n <= 262144, "gguf_metadata")?;
            let mut items = Vec::with_capacity(n as usize);
            for _ in 0..n {
                items.push(value(r, subtype, true)?);
            }
            json!(items)
        }
        10 => json!(u64::from_le_bytes(read(r)?)),
        11 => json!(i64::from_le_bytes(read(r)?)),
        12 => json!(f64::from_le_bytes(read(r)?)),
        _ => return Err(ExtractionError("gguf_metadata")),
    })
}

pub(super) fn load(file: &mut std::fs::File) -> Result<(tokenizers::Tokenizer, String)> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ExtractionError("gguf_metadata"))?;
    let mut r = file.take(32 * 1024 * 1024);
    require(&read::<4>(&mut r)? == b"GGUF", "gguf_metadata")?;
    require(u32::from_le_bytes(read(&mut r)?) == 3, "gguf_metadata")?;
    let _tensors = u64::from_le_bytes(read(&mut r)?);
    let n = u64::from_le_bytes(read(&mut r)?);
    require(n <= 1024, "gguf_metadata")?;
    let mut meta = serde_json::Map::new();
    for _ in 0..n {
        let k = string(&mut r)?;
        let ty = u32::from_le_bytes(read(&mut r)?);
        require(!meta.contains_key(&k), "gguf_metadata")?;
        meta.insert(k, value(&mut r, ty, false)?);
    }
    let meta = Value::Object(meta);
    require(
        meta["general.architecture"] == "qwen3"
            && meta["tokenizer.ggml.model"] == "gpt2"
            && meta["tokenizer.ggml.pre"] == "qwen2"
            && meta["tokenizer.ggml.add_bos_token"] == false,
        "unsupported_tokenizer",
    )?;
    let tokens = meta["tokenizer.ggml.tokens"]
        .as_array()
        .ok_or(ExtractionError("gguf_metadata"))?;
    let types = meta["tokenizer.ggml.token_type"]
        .as_array()
        .ok_or(ExtractionError("gguf_metadata"))?;
    require(tokens.len() == types.len(), "gguf_metadata")?;
    let mut vocab = serde_json::Map::new();
    let mut added = Vec::new();
    for (id, token) in tokens.iter().enumerate() {
        let token = token.as_str().ok_or(ExtractionError("gguf_metadata"))?;
        require(
            vocab.insert(token.to_owned(), json!(id)).is_none(),
            "gguf_metadata",
        )?;
        if types[id] == 3 || types[id] == 4 {
            added.push(json!({"id":id,"content":token,"single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true}));
        }
    }
    let regex = r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+";
    let spec = json!({"version":"1.0","truncation":null,"padding":null,"added_tokens":added,
        "normalizer":null,"pre_tokenizer":{"type":"Sequence","pretokenizers":[
            {"type":"Split","pattern":{"Regex":regex},"behavior":"Isolated","invert":false},
            {"type":"ByteLevel","add_prefix_space":false,"trim_offsets":false,"use_regex":false}]},
        "post_processor":null,"decoder":{"type":"ByteLevel","add_prefix_space":false,"trim_offsets":false,"use_regex":false},
        "model":{"type":"BPE","dropout":null,"unk_token":null,"continuing_subword_prefix":null,"end_of_word_suffix":null,
            "fuse_unk":false,"byte_fallback":false,"ignore_merges":false,"vocab":vocab,"merges":meta["tokenizer.ggml.merges"]}});
    let digest = artifact_sha256(&spec)?;
    let tokenizer = tokenizers::Tokenizer::from_bytes(
        serde_json::to_vec(&spec).map_err(|_| ExtractionError("tokenizer"))?,
    )
    .map_err(|_| ExtractionError("tokenizer"))?;
    Ok((tokenizer, digest))
}
