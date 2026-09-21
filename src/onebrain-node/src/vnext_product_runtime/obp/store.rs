use super::{random, Error, Result};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

const STATE: TableDefinition<&str, &[u8]> = TableDefinition::new("obp_local_commands_v1");
const MAX_STORE: usize = 16 * 1024 * 1024;
const RESULT_RESERVE: usize = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub principal: [u8; 32],
    pub operation: String,
    pub payload: Value,
    pub outcome: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub version: u32,
    pub dataset: [u8; 32],
    pub records: BTreeMap<String, Record>,
    pub configuration: Option<(bool, bool)>,
    pub sources: BTreeMap<String, (String, bool)>,
}
pub(super) struct Store {
    database: Database,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let database = Database::create(path).map_err(|_| Error::storage())?;
        let this = Self { database };
        let write = this.database.begin_write().map_err(|_| Error::storage())?;
        {
            let mut table = write.open_table(STATE).map_err(|_| Error::storage())?;
            let bytes = table
                .get("state")
                .map_err(|_| Error::storage())?
                .map(|v| v.value().to_vec());
            let mut state = match bytes {
                Some(bytes) => decode(&bytes)?,
                None => State {
                    version: 1,
                    dataset: random()?,
                    records: BTreeMap::new(),
                    configuration: None,
                    sources: BTreeMap::new(),
                },
            };
            for record in state.records.values_mut() {
                if record.outcome["state"] == "admitted" {
                    record.outcome["state"] = "reconcile_required".into();
                }
            }
            let encoded = encode(&state)?;
            table
                .insert("state", encoded.as_slice())
                .map_err(|_| Error::storage())?;
        }
        write.commit().map_err(|_| Error::storage())?;
        Ok(this)
    }
    pub fn read(&self) -> Result<State> {
        let read = self.database.begin_read().map_err(|_| Error::storage())?;
        let table = read.open_table(STATE).map_err(|_| Error::storage())?;
        let bytes = table
            .get("state")
            .map_err(|_| Error::storage())?
            .ok_or_else(Error::storage)?;
        decode(bytes.value())
    }
    pub fn mutate<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let write = self.database.begin_write().map_err(|_| Error::storage())?;
        let result;
        {
            let mut table = write.open_table(STATE).map_err(|_| Error::storage())?;
            let bytes = table
                .get("state")
                .map_err(|_| Error::storage())?
                .ok_or_else(Error::storage)?
                .value()
                .to_vec();
            let mut state = decode(&bytes)?;
            result = f(&mut state)?;
            let bytes = encode(&state)?;
            table
                .insert("state", bytes.as_slice())
                .map_err(|_| Error::storage())?;
        }
        write.commit().map_err(|_| Error::storage())?;
        Ok(result)
    }
    pub fn admit(
        &self,
        principal: [u8; 32],
        operation: &str,
        payload: &Value,
    ) -> Result<Option<Value>> {
        let key = payload["idempotency_key"]
            .as_str()
            .ok_or_else(Error::invalid)?
            .to_owned();
        self.mutate(|state| {
            if let Some(old)=state.records.get(&key) {
                if old.principal!=principal || old.operation!=operation || old.payload!=*payload {return Err(Error::new("idempotency_conflict"));}
                return Ok(Some(old.outcome.clone()));
            }
            if state.records.len()>=4096 || encode(state)?.len()+RESULT_RESERVE>MAX_STORE {return Err(Error::new("admission_limit"));}
            let outcome=json!({"idempotency_key":key,"operation":operation,"state":"admitted","reconcile_before_retry":true});
            state.records.insert(key,Record{principal,operation:operation.into(),payload:payload.clone(),outcome}); Ok(None)
        })
    }
    pub fn finish(&self, key: &str, result: Result<Value>) -> Result<Value> {
        self.mutate(|state| {
            let record=state.records.get_mut(key).ok_or_else(Error::storage)?;
            let mut outcome=json!({"idempotency_key":key,"operation":record.operation,"state":"reconcile_required","reconcile_before_retry":true});
            if let Ok(result)=result {
                let op=super::contract::inventory()["operations"].as_array().unwrap().iter().find(|o|o["name"]==record.operation).ok_or_else(Error::storage)?;
                super::contract::validate(op["response"].as_str().unwrap(),&result).map_err(|_|Error::new("response_overflow"))?;
                if serde_json::to_vec(&result).map_err(|_|Error::storage())?.len()>RESULT_RESERVE/2 {return Err(Error::new("response_overflow"));}
                outcome["state"]="completed".into();outcome["reconcile_before_retry"]=false.into();outcome["result"]=result;
            }
            record.outcome=outcome.clone();Ok(outcome)
        })
    }
}
fn encode(state: &State) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(state).map_err(|_| Error::storage())?;
    if bytes.len() > MAX_STORE {
        return Err(Error::new("admission_limit"));
    }
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> Result<State> {
    if bytes.len() > MAX_STORE {
        return Err(Error::storage());
    }
    let state: State = serde_json::from_slice(bytes).map_err(|_| Error::storage())?;
    if state.version != 1
        || state.dataset == [0; 32]
        || state.records.len() > 4096
        || state.sources.len() > 8
    {
        return Err(Error::storage());
    }
    for (key, record) in &state.records {
        super::contract::id(&Value::String(key.clone())).map_err(|_| Error::storage())?;
        super::contract::Request::new(&record.operation, record.payload.clone(), true)
            .map_err(|_| Error::storage())?;
        if record.payload["idempotency_key"] != *key
            || record.outcome["idempotency_key"] != *key
            || record.outcome["operation"] != record.operation
        {
            return Err(Error::storage());
        }
        if !matches!(
            record.outcome["state"].as_str(),
            Some("admitted" | "completed" | "failed_no_effect" | "reconcile_required")
        ) {
            return Err(Error::storage());
        }
        super::contract::fields(
            &record.outcome,
            &[
                "idempotency_key",
                "operation",
                "state",
                "reconcile_before_retry",
            ],
            &["result", "failure"],
        )
        .map_err(|_| Error::storage())?;
        let completed = record.outcome["state"] == "completed";
        let failed = record.outcome["state"] == "failed_no_effect";
        if record.outcome["reconcile_before_retry"] != (!completed && !failed)
            || record.outcome.get("result").is_some() != completed
            || record.outcome.get("failure").is_some() != failed
        {
            return Err(Error::storage());
        }
        if completed {
            let op = super::contract::inventory()["operations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|op| op["name"] == record.operation)
                .ok_or_else(Error::storage)?;
            super::contract::validate(op["response"].as_str().unwrap(), &record.outcome["result"])
                .map_err(|_| Error::storage())?;
        }
    }
    for (id, (kind, _)) in &state.sources {
        super::contract::id(&Value::String(id.clone())).map_err(|_| Error::storage())?;
        super::contract::validate("SourceKind", &Value::String(kind.clone()))
            .map_err(|_| Error::storage())?;
    }
    Ok(state)
}
