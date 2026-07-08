//! # KQL Parser — nom-based
//!
//! Parses KQL query strings into AST nodes.
//! Supports FIND, CREATE, UPDATE, DEPRECATE, WATCH, and EXPLAIN queries.

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while, take_while1},
    character::complete::{char, digit1, multispace0, multispace1},
    combinator::{map, map_res, opt, value},
    multi::separated_list1,
    sequence::{delimited, preceded, terminated, tuple},
};

use crate::ast::*;

// ═══════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════

/// Parse a KQL query string into an AST.
pub fn parse_query(input: &str) -> Result<Query, ParseError> {
    let input = input.trim();

    let (remaining, query) = query(input)
        .map_err(|e| ParseError {
            message: format!("Parse error: {:?}", e),
            position: 0,
        })?;

    let remaining = remaining.trim();
    if !remaining.is_empty() {
        return Err(ParseError {
            message: format!("Unexpected trailing input: '{}'", remaining),
            position: input.len() - remaining.len(),
        });
    }

    Ok(query)
}

/// Parse error with position information.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KQL parse error at position {}: {}", self.position, self.message)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Top-Level Parsers
// ═══════════════════════════════════════════════════════════════════════════

fn query(input: &str) -> IResult<&str, Query> {
    alt((
        map(explain_query, |q| Query::Explain(Box::new(q))),
        map(watch_query, Query::Watch),
        map(update_query, Query::Update),
        map(deprecate_query, Query::Deprecate),
        map(find_query, Query::Find),
        map(create_from_text_query, Query::CreateFromText),
        map(create_query, Query::Create),
    ))(input)
}

// ─── FIND ──────────────────────────────────────────────────────────────────

fn find_query(input: &str) -> IResult<&str, FindQuery> {
    let (input, _) = tag_no_case("FIND")(input)?;
    let (input, _) = multispace1(input)?;

    // Check for HISTORY keyword
    let (input, history) = opt(preceded(
        tag_no_case("HISTORY"),
        value(true, multispace1),
    ))(input)?;
    let history = history.unwrap_or(false);

    let (input, pattern) = pattern(input)?;
    let (input, _) = multispace0(input)?;
    let (input, where_clause) = opt(where_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, scope) = opt(scope_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, return_clause) = opt(return_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, order_by) = opt(order_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, limit) = opt(limit_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, temporal) = opt(parse_temporal_clause)(input)?;

    Ok((input, FindQuery {
        pattern,
        where_clause,
        scope: scope.unwrap_or(Scope::Auto),
        return_clause,
        limit,
        order_by,
        temporal,
        history,
    }))
}

// ─── CREATE ────────────────────────────────────────────────────────────────

fn create_query(input: &str) -> IResult<&str, CreateQuery> {
    let (input, _) = tag_no_case("CREATE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, pattern) = pattern(input)?;
    let (input, _) = multispace0(input)?;

    // Try Tier 1 structured syntax: CREATE (k:KU) FACT certainty=9000 { ... }
    if let Ok((rest, gene_type)) = gene_type_keyword(input) {
        let (rest, _) = multispace0(rest)?;
        let (rest, certainty) = opt(certainty_attr)(rest)?;
        let (rest, _) = multispace0(rest)?;
        let (rest, instructions) = instruction_block(rest)?;
        let (rest, _) = multispace0(rest)?;
        let (rest, signed) = opt(signed_clause)(rest)?;

        return Ok((rest, CreateQuery {
            properties: Vec::new(),
            pattern,
            gene_type: Some(gene_type),
            certainty,
            instructions,
            signed_by: signed.unwrap_or_default(),
        }));
    }

    // Legacy property-bag syntax: CREATE (k:KU { gene_type: "Fact", ... })
    let (input, signed) = opt(signed_clause)(input)?;

    Ok((input, CreateQuery {
        properties: pattern.nodes.first()
            .map(|n| n.properties.clone())
            .unwrap_or_default(),
        pattern,
        gene_type: None,
        certainty: None,
        instructions: Vec::new(),
        signed_by: signed.unwrap_or_default(),
    }))
}

// ─── Tier 2 CREATE FROM TEXT ───────────────────────────────────────────────

fn create_from_text_query(input: &str) -> IResult<&str, CreateFromTextQuery> {
    let (input, _) = tag_no_case("CREATE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("FROM")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("TEXT")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, text) = quoted_string(input)?;
    let (input, _) = multispace0(input)?;

    // WITH AI model="..."
    let (input, _) = tag_no_case("WITH")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("AI")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("model")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, model) = quoted_string(input)?;
    let (input, _) = multispace0(input)?;

    // Optional: gene_hint="..."
    let (input, gene_hint) = opt(|i| {
        let (i, _) = tag_no_case("gene_hint")(i)?;
        let (i, _) = multispace0(i)?;
        let (i, _) = char('=')(i)?;
        let (i, _) = multispace0(i)?;
        let (i, hint_str) = quoted_string(i)?;
        // Parse hint string to KqlGeneType
        let gene = match hint_str.to_uppercase().as_str() {
            "FACT" => KqlGeneType::Fact,
            "HYPOTHESIS" => KqlGeneType::Hypothesis,
            "EXPERIENCE" => KqlGeneType::Experience,
            "PROCEDURE" => KqlGeneType::Procedure,
            "RULE" => KqlGeneType::Rule,
            "DEFINITION" => KqlGeneType::Definition,
            "RELATION" => KqlGeneType::Relation,
            "META" => KqlGeneType::Meta,
            "CREATIVE" => KqlGeneType::Creative,
            "BELIEF" => KqlGeneType::Belief,
            "FORMALPROOF" => KqlGeneType::FormalProof,
            _ => return Err(nom::Err::Error(nom::error::Error::new(i, nom::error::ErrorKind::Tag))),
        };
        Ok((i, gene))
    })(input)?;

    let (input, _) = multispace0(input)?;
    let (input, signed) = opt(signed_clause)(input)?;

    Ok((input, CreateFromTextQuery {
        text: text.to_string(),
        model: model.to_string(),
        gene_hint,
        signed_by: signed.unwrap_or_default(),
    }))
}

// ─── Tier 1 CREATE helpers ─────────────────────────────────────────────────

fn gene_type_keyword(input: &str) -> IResult<&str, KqlGeneType> {
    alt((
        value(KqlGeneType::Fact, tag_no_case("FACT")),
        value(KqlGeneType::Hypothesis, tag_no_case("HYPOTHESIS")),
        value(KqlGeneType::Experience, tag_no_case("EXPERIENCE")),
        value(KqlGeneType::Procedure, tag_no_case("PROCEDURE")),
        value(KqlGeneType::Rule, tag_no_case("RULE")),
        value(KqlGeneType::Definition, tag_no_case("DEFINITION")),
        value(KqlGeneType::Relation, tag_no_case("RELATION")),
        value(KqlGeneType::Meta, tag_no_case("META")),
        value(KqlGeneType::Creative, tag_no_case("CREATIVE")),
        value(KqlGeneType::Belief, tag_no_case("BELIEF")),
        value(KqlGeneType::FormalProof, tag_no_case("FORMALPROOF")),
    ))(input)
}

fn certainty_attr(input: &str) -> IResult<&str, u16> {
    let (input, _) = tag_no_case("certainty")(input)?;
    let (input, _) = tag("=")(input)?;
    let (input, val) = nom::character::complete::u16(input)?;
    Ok((input, val))
}

fn instruction_block(input: &str) -> IResult<&str, Vec<CreateClause>> {
    let (input, _) = tag("{")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, clauses) = nom::multi::many0(
        delimited(multispace0, create_clause, multispace0)
    )(input)?;
    let (input, _) = tag("}")(input)?;
    Ok((input, clauses))
}

fn create_clause(input: &str) -> IResult<&str, CreateClause> {
    alt((
        triple_clause,
        quality_clause,
        quantity_clause,
        partof_clause,
        located_clause,
        temporal_clause,
        causal_clause,
        step_clause,
        precond_clause,
        effect_clause,
        certainty_clause,
        tolerance_clause,
        range_clause,
        constraint_clause,
    ))(input)
}

/// Parse a concept name: alphanumeric + underscore identifier
fn concept_name(input: &str) -> IResult<&str, String> {
    // Try quoted string first, then bare identifier
    alt((
        quoted_string,
        map(
            nom::bytes::complete::take_while1(|c: char| c.is_alphanumeric() || c == '_'),
            |s: &str| s.to_string()
        ),
    ))(input)
}

/// Parse a number (integer or float)
fn clause_number(input: &str) -> IResult<&str, f64> {
    nom::number::complete::double(input)
}

/// Comma separator with optional whitespace
fn comma_sep(input: &str) -> IResult<&str, ()> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(",")(input)?;
    let (input, _) = multispace0(input)?;
    Ok((input, ()))
}

// --- 14 clause parsers ---

fn triple_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("TRIPLE")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, s) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, p) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, o) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Triple { s, p, o }))
}

fn quality_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("QUALITY")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, s) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, q) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Quality { s, q }))
}

fn quantity_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("QUANTITY")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, s) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, v) = clause_number(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, unit) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Quantity { s, value: v, unit }))
}

fn partof_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("PARTOF")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, part) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, whole) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::PartOf { part, whole }))
}

fn located_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("LOCATED")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, s) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, location) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Located { s, location }))
}

fn temporal_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("TEMPORAL")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, s) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, time) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Temporal { s, time }))
}

fn causal_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("CAUSAL")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, cause) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, effect) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Causal { cause, effect }))
}

fn step_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("STEP")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, ord) = nom::character::complete::u8(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, action) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, target) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Step { ord, action, target }))
}

fn precond_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("PRECOND")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, concept) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Precond { concept }))
}

fn effect_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("EFFECT")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, concept) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Effect { concept }))
}

fn certainty_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("CERTAINTY")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, level) = nom::character::complete::u16(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Certainty { level }))
}

fn tolerance_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("TOLERANCE")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, s) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, v) = clause_number(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, delta) = clause_number(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Tolerance { s, value: v, delta }))
}

fn range_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("RANGE")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, s) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, min) = clause_number(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, max) = clause_number(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Range { s, min, max }))
}

fn constraint_clause(input: &str) -> IResult<&str, CreateClause> {
    let (input, _) = tag_no_case("CONSTRAINT")(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, source) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, op) = concept_name(input)?;
    let (input, _) = comma_sep(input)?;
    let (input, target) = concept_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag(")")(input)?;
    Ok((input, CreateClause::Constraint { source, op, target }))
}

// ─── WATCH ─────────────────────────────────────────────────────────────────

fn watch_query(input: &str) -> IResult<&str, WatchQuery> {
    let (input, _) = tag_no_case("WATCH")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, find) = find_query(input)?;
    let (input, _) = multispace0(input)?;
    let (input, event) = opt(on_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, notify) = opt(notify_clause)(input)?;

    Ok((input, WatchQuery {
        find,
        event: event.unwrap_or(WatchEvent::Any),
        notify: notify.unwrap_or_default(),
    }))
}

fn on_clause(input: &str) -> IResult<&str, WatchEvent> {
    let (input, _) = tag_no_case("ON")(input)?;
    let (input, _) = multispace1(input)?;
    watch_event(input)
}

fn watch_event(input: &str) -> IResult<&str, WatchEvent> {
    alt((
        value(WatchEvent::Create, tag_no_case("CREATE")),
        value(WatchEvent::Update, tag_no_case("UPDATE")),
        value(WatchEvent::Deprecate, tag_no_case("DEPRECATE")),
        value(WatchEvent::Any, tag_no_case("ANY")),
    ))(input)
}

fn notify_clause(input: &str) -> IResult<&str, String> {
    let (input, _) = tag_no_case("NOTIFY")(input)?;
    let (input, _) = multispace1(input)?;
    quoted_string(input)
}

// ─── UPDATE ───────────────────────────────────────────────────────────────

fn update_query(input: &str) -> IResult<&str, UpdateQuery> {
    let (input, _) = tag_no_case("UPDATE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, pat) = pattern(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("SET")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, assignments) = separated_list1(
        delimited(multispace0, char(','), multispace0),
        assignment,
    )(input)?;
    let (input, _) = multispace0(input)?;
    let (input, where_clause) = opt(where_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, signed) = signed_clause(input)?;

    Ok((input, UpdateQuery {
        pattern: pat,
        set_clause: assignments,
        where_clause,
        signed_by: signed,
    }))
}

fn assignment(input: &str) -> IResult<&str, Assignment> {
    let (input, field) = field_path(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, val) = parse_value(input)?;
    Ok((input, Assignment { field, value: val }))
}

// ─── DEPRECATE ────────────────────────────────────────────────────────────

fn deprecate_query(input: &str) -> IResult<&str, DeprecateQuery> {
    let (input, _) = tag_no_case("DEPRECATE")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, pat) = pattern(input)?;
    let (input, _) = multispace0(input)?;
    let (input, where_clause) = opt(where_clause)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, reason) = reason_clause(input)?;
    let (input, _) = multispace0(input)?;
    let (input, signed) = signed_clause(input)?;

    Ok((input, DeprecateQuery {
        pattern: pat,
        where_clause,
        reason,
        signed_by: signed,
    }))
}

fn reason_clause(input: &str) -> IResult<&str, String> {
    let (input, _) = tag_no_case("REASON")(input)?;
    let (input, _) = multispace1(input)?;
    quoted_string(input)
}

// ─── EXPLAIN ───────────────────────────────────────────────────────────────

fn explain_query(input: &str) -> IResult<&str, Query> {
    let (input, _) = tag_no_case("EXPLAIN")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, inner) = alt((
        map(watch_query, Query::Watch),
        map(update_query, Query::Update),
        map(deprecate_query, Query::Deprecate),
        map(find_query, Query::Find),
        map(create_query, Query::Create),
    ))(input)?;
    Ok((input, inner))
}

// ═══════════════════════════════════════════════════════════════════════════
// Pattern Parsers
// ═══════════════════════════════════════════════════════════════════════════

fn pattern(input: &str) -> IResult<&str, Pattern> {
    let (input, first_node) = node_pattern(input)?;
    let mut nodes = vec![first_node];
    let mut edges = vec![];
    let mut remaining = input;

    // Parse chains of edge + node: -[...]->(node) or <-[...]-(node)
    loop {
        let trimmed = remaining.trim_start();
        match edge_pattern(trimmed, nodes.len() - 1) {
            Ok((rest, edge)) => {
                let rest = rest.trim_start();
                match node_pattern(rest) {
                    Ok((rest2, node)) => {
                        let mut e = edge;
                        e.to = nodes.len();
                        edges.push(e);
                        nodes.push(node);
                        remaining = rest2;
                    }
                    Err(_) => break,
                }
            }
            Err(_) => break,
        }
    }

    Ok((remaining, Pattern { nodes, edges }))
}

/// Parse an edge pattern: `-[alias:Type1|Type2]->` or `<-[alias:Type1|Type2]-`
fn edge_pattern(input: &str, from_idx: usize) -> IResult<&str, EdgePattern> {
    // Detect direction prefix
    let (input, direction) = if input.starts_with("<-[") {
        (&input[2..], EdgeDirection::Incoming)  // skip "<-", keep "["
    } else if input.starts_with("-[") {
        (&input[1..], EdgeDirection::Outgoing)  // skip "-", keep "["
    } else {
        return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag)));
    };

    // Parse [...] content
    let (input, _) = tag("[")(input)?;
    let (input, _) = multispace0(input)?;

    // Optional path depth: *1..3
    let (input, path_depth) = opt(parse_path_depth)(input)?;

    // Optional alias and types
    let (input, _) = multispace0(input)?;
    let (input, (alias, edge_types)) = parse_edge_label(input)?;

    let (input, _) = multispace0(input)?;
    let (input, _) = tag("]")(input)?;

    // Direction suffix
    let input = if direction == EdgeDirection::Outgoing {
        let (input, _) = tag("->")(input)?;
        input
    } else {
        let (input, _) = tag("-")(input)?;
        input
    };

    Ok((input, EdgePattern {
        alias,
        edge_types,
        direction,
        from: from_idx,
        to: 0, // will be set by pattern()
        path_depth,
    }))
}

/// Parse `*min..max` path depth (e.g., `*1..3`, `*2..5`)
fn parse_path_depth(input: &str) -> IResult<&str, PathDepth> {
    let (input, _) = tag("*")(input)?;
    let (input, min) = nom::character::complete::u32(input)?;
    let (input, _) = tag("..")(input)?;
    let (input, max) = nom::character::complete::u32(input)?;
    Ok((input, PathDepth { min: min as usize, max: max as usize }))
}

/// Parse an edge alias: identifier followed by `:`
fn parse_edge_alias(input: &str) -> IResult<&str, String> {
    let (input, name) = identifier(input)?;
    let (input, _) = tag(":")(input)?;
    Ok((input, name.to_string()))
}

/// Parse optional `alias:Type1|Type2` inside edge brackets.
fn parse_edge_label(input: &str) -> IResult<&str, (Option<String>, Vec<String>)> {
    // Empty bracket: -[]->
    if input.starts_with(']') {
        return Ok((input, (None, vec![])));
    }

    // Check for alias (identifier followed by ':')
    let (rest, alias) = opt(parse_edge_alias)(input)?;

    // If no alias, check for bare ':'
    let (rest, _) = if alias.is_none() {
        opt(tag(":"))(rest)?
    } else {
        (rest, None)
    };

    // Parse pipe-separated type names
    let (rest, types) = parse_edge_types(rest)?;

    Ok((rest, (alias, types)))
}

/// Parse pipe-separated edge type names (e.g., `Extends|Supplements`).
fn parse_edge_types(input: &str) -> IResult<&str, Vec<String>> {
    if input.starts_with(']') {
        return Ok((input, vec![]));
    }
    let (input, first) = identifier(input)?;
    let mut types = vec![first.to_string()];
    let mut remaining = input;
    loop {
        match tag::<&str, &str, nom::error::Error<&str>>("|") (remaining) {
            Ok((rest, _)) => {
                match identifier(rest) {
                    Ok((rest2, next)) => {
                        types.push(next.to_string());
                        remaining = rest2;
                    }
                    Err(_) => break,
                }
            }
            Err(_) => break,
        }
    }
    Ok((remaining, types))
}

/// Parse temporal clause: `AT TIME <ts>` or `DURING <from> <to>`
fn parse_temporal_clause(input: &str) -> IResult<&str, TemporalClause> {
    let input = input.trim_start();
    // Try AT TIME <timestamp>
    if let Ok((rest, _)) = tag_no_case::<&str, &str, nom::error::Error<&str>>("AT")(input) {
        let (rest, _) = multispace1(rest)?;
        let (rest, _) = tag_no_case("TIME")(rest)?;
        let (rest, _) = multispace1(rest)?;
        let (rest, ts) = nom::character::complete::u64(rest)?;
        return Ok((rest, TemporalClause::AtTime(ts)));
    }
    // Try DURING <from> <to>
    let (input, _) = tag_no_case("DURING")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, from) = nom::character::complete::u64(input)?;
    let (input, _) = multispace1(input)?;
    let (input, to) = nom::character::complete::u64(input)?;
    Ok((input, TemporalClause::During { from, to }))
}

fn node_pattern(input: &str) -> IResult<&str, NodePattern> {
    let (input, _) = char('(')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, alias) = opt(terminated(identifier, char(':')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, label) = node_label(input)?;
    let (input, _) = multispace0(input)?;
    let (input, props) = opt(property_map)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(')')(input)?;

    Ok((input, NodePattern {
        alias: alias.map(|s| s.to_string()),
        label,
        properties: props.unwrap_or_default(),
    }))
}

fn node_label(input: &str) -> IResult<&str, NodeLabel> {
    alt((
        value(NodeLabel::KU, tag_no_case("KU")),
        value(NodeLabel::Concept, tag_no_case("Concept")),
    ))(input)
}

fn property_map(input: &str) -> IResult<&str, Vec<Property>> {
    delimited(
        char('{'),
        separated_list1(
            delimited(multispace0, char(','), multispace0),
            property,
        ),
        preceded(multispace0, char('}')),
    )(input)
}

fn property(input: &str) -> IResult<&str, Property> {
    let (input, _) = multispace0(input)?;
    let (input, key) = identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(':')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, val) = parse_value(input)?;

    Ok((input, Property {
        key: key.to_string(),
        value: val,
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// WHERE Clause
// ═══════════════════════════════════════════════════════════════════════════

fn where_clause(input: &str) -> IResult<&str, Condition> {
    let (input, _) = tag_no_case("WHERE")(input)?;
    let (input, _) = multispace1(input)?;
    condition(input)
}

fn condition(input: &str) -> IResult<&str, Condition> {
    let (input, left) = simple_condition(input)?;
    let (input, _) = multispace0(input)?;

    // Try AND/OR continuation
    if let Ok((input, _)) = tag_no_case::<&str, &str, nom::error::Error<&str>>("AND")(input) {
        let (input, _) = multispace1(input)?;
        let (input, right) = condition(input)?;
        return Ok((input, Condition::And(Box::new(left), Box::new(right))));
    }

    if let Ok((input, _)) = tag_no_case::<&str, &str, nom::error::Error<&str>>("OR")(input) {
        let (input, _) = multispace1(input)?;
        let (input, right) = condition(input)?;
        return Ok((input, Condition::Or(Box::new(left), Box::new(right))));
    }

    Ok((input, left))
}

fn simple_condition(input: &str) -> IResult<&str, Condition> {
    alt((
        not_condition,
        exists_condition,
        contains_condition,
        comparison_condition,
    ))(input)
}

/// Parse `NOT <condition>` — wraps the inner condition in `Condition::Not`.
fn not_condition(input: &str) -> IResult<&str, Condition> {
    let (input, _) = tag_no_case("NOT")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, inner) = simple_condition(input)?;
    Ok((input, Condition::Not(Box::new(inner))))
}

fn exists_condition(input: &str) -> IResult<&str, Condition> {
    let (input, _) = tag_no_case("EXISTS")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, field) = field_path(input)?;
    Ok((input, Condition::Exists(field)))
}

fn contains_condition(input: &str) -> IResult<&str, Condition> {
    let (input, field) = field_path(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("CONTAINS")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, val) = parse_value(input)?;
    Ok((input, Condition::Contains { field, value: val }))
}

fn comparison_condition(input: &str) -> IResult<&str, Condition> {
    let (input, field) = field_path(input)?;
    let (input, _) = multispace0(input)?;
    let (input, op) = comp_op(input)?;
    let (input, _) = multispace0(input)?;
    let (input, val) = parse_value(input)?;

    Ok((input, Condition::Comparison { field, op, value: val }))
}

fn comp_op(input: &str) -> IResult<&str, CompOp> {
    alt((
        value(CompOp::NotEq, tag("!=")),
        value(CompOp::GtEq, tag(">=")),
        value(CompOp::LtEq, tag("<=")),
        value(CompOp::Eq, tag("=")),
        value(CompOp::Gt, tag(">")),
        value(CompOp::Lt, tag("<")),
    ))(input)
}

// ═══════════════════════════════════════════════════════════════════════════
// SCOPE / RETURN / ORDER / LIMIT Clauses
// ═══════════════════════════════════════════════════════════════════════════

fn scope_clause(input: &str) -> IResult<&str, Scope> {
    let (input, _) = tag_no_case("SCOPE")(input)?;
    let (input, _) = multispace1(input)?;
    alt((
        value(Scope::Local, tag_no_case("LOCAL")),
        value(Scope::Neighbors, tag_no_case("NEIGHBORS")),
        value(Scope::Cluster, tag_no_case("CLUSTER")),
        value(Scope::Dht, tag_no_case("DHT")),
        value(Scope::Semantic, tag_no_case("SEMANTIC")),
        value(Scope::Global, tag_no_case("GLOBAL")),
        value(Scope::Auto, tag_no_case("AUTO")),
    ))(input)
}

fn return_clause(input: &str) -> IResult<&str, Vec<ReturnExpr>> {
    let (input, _) = tag_no_case("RETURN")(input)?;
    let (input, _) = multispace1(input)?;
    separated_list1(
        delimited(multispace0, char(','), multispace0),
        return_expr,
    )(input)
}

fn return_expr(input: &str) -> IResult<&str, ReturnExpr> {
    alt((
        aggregate_expr,
        map(field_path, ReturnExpr::Field),
        map(identifier, |s| ReturnExpr::Alias(s.to_string())),
    ))(input)
}

fn aggregate_expr(input: &str) -> IResult<&str, ReturnExpr> {
    let (input, func) = agg_func(input)?;
    let (input, _) = char('(')(input)?;
    let (input, field) = field_path(input)?;
    let (input, _) = char(')')(input)?;
    let (input, alias) = opt(preceded(
        tuple((multispace1, tag_no_case("AS"), multispace1)),
        identifier,
    ))(input)?;

    Ok((input, ReturnExpr::Aggregate {
        func,
        field,
        alias: alias.map(|s| s.to_string()),
    }))
}

fn agg_func(input: &str) -> IResult<&str, AggFunc> {
    alt((
        value(AggFunc::Count, tag_no_case("COUNT")),
        value(AggFunc::Sum, tag_no_case("SUM")),
        value(AggFunc::Avg, tag_no_case("AVG")),
        value(AggFunc::Min, tag_no_case("MIN")),
        value(AggFunc::Max, tag_no_case("MAX")),
    ))(input)
}

fn order_clause(input: &str) -> IResult<&str, Vec<OrderExpr>> {
    let (input, _) = tag_no_case("ORDER")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("BY")(input)?;
    let (input, _) = multispace1(input)?;
    separated_list1(
        delimited(multispace0, char(','), multispace0),
        order_expr,
    )(input)
}

fn order_expr(input: &str) -> IResult<&str, OrderExpr> {
    let (input, field) = field_path(input)?;
    let (input, _) = multispace0(input)?;
    let (input, desc) = opt(alt((
        value(true, tag_no_case("DESC")),
        value(false, tag_no_case("ASC")),
    )))(input)?;

    Ok((input, OrderExpr {
        field,
        descending: desc.unwrap_or(false),
    }))
}

fn limit_clause(input: &str) -> IResult<&str, u32> {
    let (input, _) = tag_no_case("LIMIT")(input)?;
    let (input, _) = multispace1(input)?;
    map_res(digit1, |s: &str| s.parse::<u32>())(input)
}

fn signed_clause(input: &str) -> IResult<&str, String> {
    let (input, _) = tag_no_case("SIGNED")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("BY")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, signer) = alt((
        quoted_string,
        map(identifier, |s| s.to_string()),
    ))(input)?;
    Ok((input, signer))
}

// ═══════════════════════════════════════════════════════════════════════════
// Primitives
// ═══════════════════════════════════════════════════════════════════════════

fn identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)
}

fn field_path(input: &str) -> IResult<&str, FieldPath> {
    let (input, segments) = separated_list1(char('.'), identifier)(input)?;
    Ok((input, FieldPath::new(segments.into_iter().map(String::from).collect())))
}

fn parse_value(input: &str) -> IResult<&str, Value> {
    alt((
        map(quoted_string, Value::Text),
        value(Value::Bool(true), tag_no_case("true")),
        value(Value::Bool(false), tag_no_case("false")),
        parse_number,
    ))(input)
}

fn parse_number(input: &str) -> IResult<&str, Value> {
    let (input, neg) = opt(char('-'))(input)?;
    let (input, digits) = digit1(input)?;

    // Check for float
    if let Ok((input, _)) = char::<&str, nom::error::Error<&str>>('.')(input) {
        let (input, frac) = digit1(input)?;
        let s = format!("{}{}.{}", if neg.is_some() { "-" } else { "" }, digits, frac);
        let f: f64 = s.parse().unwrap_or(0.0);
        return Ok((input, Value::Float(f)));
    }

    let s = format!("{}{}", if neg.is_some() { "-" } else { "" }, digits);
    let i: i64 = s.parse().unwrap_or(0);
    Ok((input, Value::Integer(i)))
}

fn quoted_string(input: &str) -> IResult<&str, String> {
    let (input, _) = char('"')(input)?;
    let (input, content) = take_while(|c: char| c != '"')(input)?;
    let (input, _) = char('"')(input)?;
    Ok((input, content.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_find() {
        let q = parse_query("FIND (k:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.nodes.len(), 1);
                assert_eq!(f.pattern.nodes[0].alias, Some("k".to_string()));
                assert_eq!(f.pattern.nodes[0].label, NodeLabel::KU);
                assert_eq!(f.scope, Scope::Auto);
            },
            _ => panic!("Expected Find query"),
        }
    }

    #[test]
    fn test_parse_find_with_where() {
        let q = parse_query("FIND (k:KU) WHERE k.trust_score > 8000").unwrap();
        match q {
            Query::Find(f) => {
                assert!(f.where_clause.is_some());
                match f.where_clause.unwrap() {
                    Condition::Comparison { field, op, value } => {
                        assert_eq!(field.segments, vec!["k", "trust_score"]);
                        assert_eq!(op, CompOp::Gt);
                        assert_eq!(value, Value::Integer(8000));
                    },
                    _ => panic!("Expected comparison"),
                }
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_scope_and_limit() {
        let q = parse_query("FIND (k:KU) SCOPE LOCAL LIMIT 10").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.scope, Scope::Local);
                assert_eq!(f.limit, Some(10));
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_and_condition() {
        let q = parse_query(
            "FIND (k:KU) WHERE k.trust_score > 5000 AND k.certainty >= 9000"
        ).unwrap();
        match q {
            Query::Find(f) => {
                match f.where_clause.unwrap() {
                    Condition::And(left, right) => {
                        assert!(matches!(*left, Condition::Comparison { .. }));
                        assert!(matches!(*right, Condition::Comparison { .. }));
                    },
                    _ => panic!("Expected AND"),
                }
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_return() {
        let q = parse_query(
            "FIND (k:KU) RETURN k.trust_score, k.certainty"
        ).unwrap();
        match q {
            Query::Find(f) => {
                let ret = f.return_clause.unwrap();
                assert_eq!(ret.len(), 2);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_order() {
        let q = parse_query(
            "FIND (k:KU) ORDER BY k.trust_score DESC LIMIT 5"
        ).unwrap();
        match q {
            Query::Find(f) => {
                let order = f.order_by.unwrap();
                assert_eq!(order.len(), 1);
                assert!(order[0].descending);
                assert_eq!(f.limit, Some(5));
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_properties() {
        let q = parse_query(
            r#"FIND (k:KU {gene_type: "Fact", certainty: 9500})"#
        ).unwrap();
        match q {
            Query::Find(f) => {
                let props = &f.pattern.nodes[0].properties;
                assert_eq!(props.len(), 2);
                assert_eq!(props[0].key, "gene_type");
                assert_eq!(props[0].value, Value::Text("Fact".to_string()));
                assert_eq!(props[1].key, "certainty");
                assert_eq!(props[1].value, Value::Integer(9500));
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_create() {
        let q = parse_query(
            r#"CREATE (k:KU {gene_type: "Fact"}) SIGNED BY "did:key:z6Mk...""#
        ).unwrap();
        match q {
            Query::Create(c) => {
                assert_eq!(c.pattern.nodes[0].label, NodeLabel::KU);
                assert!(!c.properties.is_empty());
                assert!(c.signed_by.starts_with("did:key"));
            },
            _ => panic!("Expected Create"),
        }
    }

    #[test]
    fn test_parse_explain() {
        let q = parse_query("EXPLAIN FIND (k:KU) SCOPE DHT").unwrap();
        match q {
            Query::Explain(inner) => {
                match *inner {
                    Query::Find(f) => assert_eq!(f.scope, Scope::Dht),
                    _ => panic!("Expected Find inside Explain"),
                }
            },
            _ => panic!("Expected Explain"),
        }
    }

    #[test]
    fn test_parse_aggregate() {
        let q = parse_query(
            "FIND (k:KU) RETURN COUNT(k.id) AS total"
        ).unwrap();
        match q {
            Query::Find(f) => {
                let ret = f.return_clause.unwrap();
                assert_eq!(ret.len(), 1);
                match &ret[0] {
                    ReturnExpr::Aggregate { func, alias, .. } => {
                        assert_eq!(*func, AggFunc::Count);
                        assert_eq!(alias.as_deref(), Some("total"));
                    },
                    _ => panic!("Expected Aggregate"),
                }
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_error_invalid() {
        let result = parse_query("SELECT * FROM ku");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_concept_label() {
        let q = parse_query("FIND (c:Concept)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.nodes[0].label, NodeLabel::Concept);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_exists_condition() {
        let q = parse_query("FIND (k:KU) WHERE EXISTS k.trust").unwrap();
        match q {
            Query::Find(f) => {
                match f.where_clause.unwrap() {
                    Condition::Exists(field) => {
                        assert_eq!(field.segments, vec!["k", "trust"]);
                    },
                    _ => panic!("Expected Exists"),
                }
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_negative_number() {
        let q = parse_query("FIND (k:KU) WHERE k.score > -100").unwrap();
        match q {
            Query::Find(f) => {
                match f.where_clause.unwrap() {
                    Condition::Comparison { value, .. } => {
                        assert_eq!(value, Value::Integer(-100));
                    },
                    _ => panic!("Expected comparison"),
                }
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_float_value() {
        let q = parse_query("FIND (k:KU) WHERE k.confidence > 0.95").unwrap();
        match q {
            Query::Find(f) => {
                match f.where_clause.unwrap() {
                    Condition::Comparison { value, .. } => {
                        assert_eq!(value, Value::Float(0.95));
                    },
                    _ => panic!("Expected comparison"),
                }
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_all_scopes() {
        for (scope_str, expected) in [
            ("LOCAL", Scope::Local),
            ("NEIGHBORS", Scope::Neighbors),
            ("CLUSTER", Scope::Cluster),
            ("DHT", Scope::Dht),
            ("SEMANTIC", Scope::Semantic),
            ("GLOBAL", Scope::Global),
            ("AUTO", Scope::Auto),
        ] {
            let q = parse_query(&format!("FIND (k:KU) SCOPE {}", scope_str)).unwrap();
            match q {
                Query::Find(f) => assert_eq!(f.scope, expected, "Scope: {}", scope_str),
                _ => panic!("Expected Find"),
            }
        }
    }

    // ─── WATCH tests ───────────────────────────────────────────────────────

    #[test]
    fn test_parse_watch_simple() {
        let q = parse_query("WATCH FIND (k:KU)").unwrap();
        match q {
            Query::Watch(w) => {
                assert_eq!(w.find.pattern.nodes[0].label, NodeLabel::KU);
                assert_eq!(w.event, WatchEvent::Any);
                assert_eq!(w.notify, "");
            },
            _ => panic!("Expected Watch query"),
        }
    }

    #[test]
    fn test_parse_watch_full() {
        let q = parse_query(
            r#"WATCH FIND (k:KU) WHERE k.trust_score > 5000 ON CREATE NOTIFY "https://hook.example.com""#
        ).unwrap();
        match q {
            Query::Watch(w) => {
                assert!(w.find.where_clause.is_some());
                assert_eq!(w.event, WatchEvent::Create);
                assert_eq!(w.notify, "https://hook.example.com");
            },
            _ => panic!("Expected Watch query"),
        }
    }

    #[test]
    fn test_parse_watch_on_update() {
        let q = parse_query("WATCH FIND (c:Concept) ON UPDATE").unwrap();
        match q {
            Query::Watch(w) => {
                assert_eq!(w.event, WatchEvent::Update);
                assert_eq!(w.notify, "");
            },
            _ => panic!("Expected Watch query"),
        }
    }

    #[test]
    fn test_parse_watch_on_deprecate() {
        let q = parse_query(
            r#"WATCH FIND (k:KU) ON DEPRECATE NOTIFY "grpc://alerts:50051""#
        ).unwrap();
        match q {
            Query::Watch(w) => {
                assert_eq!(w.event, WatchEvent::Deprecate);
                assert_eq!(w.notify, "grpc://alerts:50051");
            },
            _ => panic!("Expected Watch query"),
        }
    }

    // ─── UPDATE tests ──────────────────────────────────────────────────────

    #[test]
    fn test_parse_update_single_set() {
        let q = parse_query(
            r#"UPDATE (k:KU) SET k.trust_score = 9000 SIGNED BY "did:key:z6Mk...""#
        ).unwrap();
        match q {
            Query::Update(u) => {
                assert_eq!(u.set_clause.len(), 1);
                assert_eq!(u.set_clause[0].field.segments, vec!["k", "trust_score"]);
                assert_eq!(u.set_clause[0].value, Value::Integer(9000));
                assert!(u.where_clause.is_none());
                assert!(u.signed_by.starts_with("did:key"));
            },
            _ => panic!("Expected Update query"),
        }
    }

    #[test]
    fn test_parse_update_multi_set_with_where() {
        let q = parse_query(
            r#"UPDATE (k:KU) SET k.trust_score = 9500, k.certainty = 9800 WHERE k.id = "abc" SIGNED BY "alice""#
        ).unwrap();
        match q {
            Query::Update(u) => {
                assert_eq!(u.set_clause.len(), 2);
                assert_eq!(u.set_clause[0].value, Value::Integer(9500));
                assert_eq!(u.set_clause[1].field.segments, vec!["k", "certainty"]);
                assert_eq!(u.set_clause[1].value, Value::Integer(9800));
                assert!(u.where_clause.is_some());
                assert_eq!(u.signed_by, "alice");
            },
            _ => panic!("Expected Update query"),
        }
    }

    #[test]
    fn test_parse_update_string_value() {
        let q = parse_query(
            r#"UPDATE (k:KU) SET k.title = "new title" SIGNED BY "bob""#
        ).unwrap();
        match q {
            Query::Update(u) => {
                assert_eq!(u.set_clause[0].value, Value::Text("new title".to_string()));
            },
            _ => panic!("Expected Update query"),
        }
    }

    // ─── DEPRECATE tests ───────────────────────────────────────────────────

    #[test]
    fn test_parse_deprecate_simple() {
        let q = parse_query(
            r#"DEPRECATE (k:KU) REASON "outdated information" SIGNED BY "admin""#
        ).unwrap();
        match q {
            Query::Deprecate(d) => {
                assert_eq!(d.pattern.nodes[0].label, NodeLabel::KU);
                assert!(d.where_clause.is_none());
                assert_eq!(d.reason, "outdated information");
                assert_eq!(d.signed_by, "admin");
            },
            _ => panic!("Expected Deprecate query"),
        }
    }

    #[test]
    fn test_parse_deprecate_with_where() {
        let q = parse_query(
            r#"DEPRECATE (k:KU) WHERE k.trust_score < 1000 REASON "low trust" SIGNED BY "did:key:z6Mk...""#
        ).unwrap();
        match q {
            Query::Deprecate(d) => {
                assert!(d.where_clause.is_some());
                match d.where_clause.unwrap() {
                    Condition::Comparison { field, op, value } => {
                        assert_eq!(field.segments, vec!["k", "trust_score"]);
                        assert_eq!(op, CompOp::Lt);
                        assert_eq!(value, Value::Integer(1000));
                    },
                    _ => panic!("Expected comparison"),
                }
                assert_eq!(d.reason, "low trust");
                assert!(d.signed_by.starts_with("did:key"));
            },
            _ => panic!("Expected Deprecate query"),
        }
    }

    // ─── EXPLAIN new query types ───────────────────────────────────────────

    #[test]
    fn test_parse_explain_watch() {
        let q = parse_query("EXPLAIN WATCH FIND (k:KU) ON ANY").unwrap();
        match q {
            Query::Explain(inner) => {
                match *inner {
                    Query::Watch(w) => {
                        assert_eq!(w.event, WatchEvent::Any);
                    },
                    _ => panic!("Expected Watch inside Explain"),
                }
            },
            _ => panic!("Expected Explain"),
        }
    }

    #[test]
    fn test_parse_explain_update() {
        let q = parse_query(
            r#"EXPLAIN UPDATE (k:KU) SET k.score = 100 SIGNED BY "alice""#
        ).unwrap();
        match q {
            Query::Explain(inner) => {
                match *inner {
                    Query::Update(u) => {
                        assert_eq!(u.set_clause.len(), 1);
                    },
                    _ => panic!("Expected Update inside Explain"),
                }
            },
            _ => panic!("Expected Explain"),
        }
    }

    #[test]
    fn test_parse_explain_deprecate() {
        let q = parse_query(
            r#"EXPLAIN DEPRECATE (k:KU) REASON "test" SIGNED BY "bob""#
        ).unwrap();
        match q {
            Query::Explain(inner) => {
                match *inner {
                    Query::Deprecate(d) => {
                        assert_eq!(d.reason, "test");
                    },
                    _ => panic!("Expected Deprecate inside Explain"),
                }
            },
            _ => panic!("Expected Explain"),
        }
    }

    // ─── Tier 1 Structured CREATE Parser Tests ─────────────────────────

    #[test]
    fn test_parse_tier1_create_fact() {
        let q = parse_query(
            r#"CREATE (k:KU) FACT certainty=9000 { TRIPLE(water, boils_at, 100_celsius) }"#
        ).unwrap();
        match q {
            Query::Create(c) => {
                assert_eq!(c.gene_type, Some(KqlGeneType::Fact));
                assert_eq!(c.certainty, Some(9000));
                assert_eq!(c.instructions.len(), 1);
                match &c.instructions[0] {
                    CreateClause::Triple { s, p, o } => {
                        assert_eq!(s, "water");
                        assert_eq!(p, "boils_at");
                        assert_eq!(o, "100_celsius");
                    },
                    _ => panic!("Expected Triple"),
                }
            },
            _ => panic!("Expected Create"),
        }
    }

    #[test]
    fn test_parse_tier1_create_procedure() {
        let q = parse_query(
            r#"CREATE (k:KU) PROCEDURE certainty=7000 {
                STEP(1, enter, water)
                STEP(2, kick, legs)
                PRECOND(know_swimming)
                EFFECT(move_forward)
            }"#
        ).unwrap();
        match q {
            Query::Create(c) => {
                assert_eq!(c.gene_type, Some(KqlGeneType::Procedure));
                assert_eq!(c.certainty, Some(7000));
                assert_eq!(c.instructions.len(), 4);
                assert!(matches!(&c.instructions[0], CreateClause::Step { ord: 1, .. }));
                assert!(matches!(&c.instructions[2], CreateClause::Precond { .. }));
                assert!(matches!(&c.instructions[3], CreateClause::Effect { .. }));
            },
            _ => panic!("Expected Create"),
        }
    }

    #[test]
    fn test_parse_tier1_all_clause_types() {
        let q = parse_query(
            r#"CREATE (k:KU) FACT {
                TRIPLE(a, b, c)
                QUALITY(x, good)
                QUANTITY(mass, 5.0, kg)
                PARTOF(wing, bird)
                LOCATED(tree, forest)
                TEMPORAL(event, summer)
                CAUSAL(rain, flood)
                TOLERANCE(temp, 100.0, 0.5)
                RANGE(ph, 6.5, 7.5)
                CONSTRAINT(age, gt, min_age)
            }"#
        ).unwrap();
        match q {
            Query::Create(c) => {
                assert_eq!(c.instructions.len(), 10);
                assert!(matches!(&c.instructions[0], CreateClause::Triple { .. }));
                assert!(matches!(&c.instructions[1], CreateClause::Quality { .. }));
                assert!(matches!(&c.instructions[2], CreateClause::Quantity { .. }));
                assert!(matches!(&c.instructions[3], CreateClause::PartOf { .. }));
                assert!(matches!(&c.instructions[4], CreateClause::Located { .. }));
                assert!(matches!(&c.instructions[5], CreateClause::Temporal { .. }));
                assert!(matches!(&c.instructions[6], CreateClause::Causal { .. }));
                assert!(matches!(&c.instructions[7], CreateClause::Tolerance { .. }));
                assert!(matches!(&c.instructions[8], CreateClause::Range { .. }));
                assert!(matches!(&c.instructions[9], CreateClause::Constraint { .. }));
            },
            _ => panic!("Expected Create"),
        }
    }

    #[test]
    fn test_parse_tier1_no_certainty() {
        let q = parse_query(
            r#"CREATE (k:KU) HYPOTHESIS { TRIPLE(idea, might_cause, result) }"#
        ).unwrap();
        match q {
            Query::Create(c) => {
                assert_eq!(c.gene_type, Some(KqlGeneType::Hypothesis));
                assert_eq!(c.certainty, None);
                assert_eq!(c.instructions.len(), 1);
            },
            _ => panic!("Expected Create"),
        }
    }

    #[test]
    fn test_parse_tier1_quoted_concept_names() {
        let q = parse_query(
            r#"CREATE (k:KU) FACT { TRIPLE("water molecule", "boils at", "100 celsius") }"#
        ).unwrap();
        match q {
            Query::Create(c) => {
                match &c.instructions[0] {
                    CreateClause::Triple { s, p, o } => {
                        assert_eq!(s, "water molecule");
                        assert_eq!(p, "boils at");
                        assert_eq!(o, "100 celsius");
                    },
                    _ => panic!("Expected Triple"),
                }
            },
            _ => panic!("Expected Create"),
        }
    }

    #[test]
    fn test_parse_tier1_all_gene_types() {
        let types = vec![
            ("FACT", KqlGeneType::Fact),
            ("HYPOTHESIS", KqlGeneType::Hypothesis),
            ("EXPERIENCE", KqlGeneType::Experience),
            ("PROCEDURE", KqlGeneType::Procedure),
            ("RULE", KqlGeneType::Rule),
            ("DEFINITION", KqlGeneType::Definition),
            ("RELATION", KqlGeneType::Relation),
            ("META", KqlGeneType::Meta),
            ("CREATIVE", KqlGeneType::Creative),
            ("BELIEF", KqlGeneType::Belief),
            ("FORMALPROOF", KqlGeneType::FormalProof),
        ];
        for (kw, expected) in types {
            let qs = format!(r#"CREATE (k:KU) {} {{ TRIPLE(a, b, c) }}"#, kw);
            let q = parse_query(&qs).unwrap();
            match q {
                Query::Create(c) => assert_eq!(c.gene_type, Some(expected), "Failed for {}", kw),
                _ => panic!("Expected Create for {}", kw),
            }
        }
    }

    // ─── Phase 3: Edge pattern tests ────────────────────────────────────

    #[test]
    fn test_parse_find_with_edge_outgoing() {
        let q = parse_query("FIND (k:KU)-[:Extends]->(m:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.nodes.len(), 2);
                assert_eq!(f.pattern.edges.len(), 1);
                assert_eq!(f.pattern.edges[0].direction, EdgeDirection::Outgoing);
                assert_eq!(f.pattern.edges[0].edge_types, vec!["Extends"]);
                assert_eq!(f.pattern.edges[0].from, 0);
                assert_eq!(f.pattern.edges[0].to, 1);
                assert!(f.pattern.edges[0].alias.is_none());
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_edge_incoming() {
        let q = parse_query("FIND (k:KU)<-[:Causes]-(m:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.nodes.len(), 2);
                assert_eq!(f.pattern.edges.len(), 1);
                assert_eq!(f.pattern.edges[0].direction, EdgeDirection::Incoming);
                assert_eq!(f.pattern.edges[0].edge_types, vec!["Causes"]);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_edge_alias() {
        let q = parse_query("FIND (k:KU)-[r:Extends]->(m:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.edges[0].alias, Some("r".to_string()));
                assert_eq!(f.pattern.edges[0].edge_types, vec!["Extends"]);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_multiple_edge_types() {
        let q = parse_query("FIND (k:KU)-[:Extends|Supplements]->(m:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.edges[0].edge_types, vec!["Extends", "Supplements"]);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_with_path_depth() {
        let q = parse_query("FIND (k:KU)-[*1..3:Extends]->(m:KU)").unwrap();
        match q {
            Query::Find(f) => {
                let depth = f.pattern.edges[0].path_depth.as_ref().unwrap();
                assert_eq!(depth.min, 1);
                assert_eq!(depth.max, 3);
                assert_eq!(f.pattern.edges[0].edge_types, vec!["Extends"]);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_chain_two_edges() {
        let q = parse_query("FIND (a:KU)-[:Extends]->(b:KU)-[:Causes]->(c:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.nodes.len(), 3);
                assert_eq!(f.pattern.edges.len(), 2);
                assert_eq!(f.pattern.edges[0].from, 0);
                assert_eq!(f.pattern.edges[0].to, 1);
                assert_eq!(f.pattern.edges[0].edge_types, vec!["Extends"]);
                assert_eq!(f.pattern.edges[1].from, 1);
                assert_eq!(f.pattern.edges[1].to, 2);
                assert_eq!(f.pattern.edges[1].edge_types, vec!["Causes"]);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_empty_edge() {
        let q = parse_query("FIND (k:KU)-[]->(m:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.edges[0].edge_types.len(), 0);
                assert!(f.pattern.edges[0].alias.is_none());
            },
            _ => panic!("Expected Find"),
        }
    }

    // ─── Phase 3: Temporal parsing tests ────────────────────────────────

    #[test]
    fn test_parse_find_at_time() {
        let q = parse_query("FIND (k:KU) AT TIME 1719900000").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.temporal, Some(TemporalClause::AtTime(1719900000)));
                assert!(!f.history);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_during() {
        let q = parse_query("FIND (k:KU) DURING 1719800000 1719900000").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.temporal, Some(TemporalClause::During { from: 1719800000, to: 1719900000 }));
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_history() {
        let q = parse_query("FIND HISTORY (k:KU)").unwrap();
        match q {
            Query::Find(f) => {
                assert!(f.history);
                assert_eq!(f.pattern.nodes[0].alias, Some("k".to_string()));
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_history_with_where() {
        let q = parse_query("FIND HISTORY (k:KU) WHERE k.trust_score > 5000").unwrap();
        match q {
            Query::Find(f) => {
                assert!(f.history);
                assert!(f.where_clause.is_some());
            },
            _ => panic!("Expected Find"),
        }
    }

    // ─── Phase 3: Combined edge + temporal tests ───────────────────────

    #[test]
    fn test_parse_find_edge_at_time() {
        let q = parse_query("FIND (k:KU)-[:Extends]->(m:KU) AT TIME 1719900000").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.edges.len(), 1);
                assert_eq!(f.temporal, Some(TemporalClause::AtTime(1719900000)));
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_edge_with_where_and_temporal() {
        let q = parse_query(
            "FIND (k:KU)-[:Extends]->(m:KU) WHERE k.trust_score > 5000 DURING 1719800000 1719900000"
        ).unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.edges.len(), 1);
                assert!(f.where_clause.is_some());
                assert_eq!(f.temporal, Some(TemporalClause::During { from: 1719800000, to: 1719900000 }));
            },
            _ => panic!("Expected Find"),
        }
    }

    // ─── Phase 3: Backward compatibility ───────────────────────────────

    #[test]
    fn test_parse_simple_find_still_works() {
        let q = parse_query("FIND (k:KU) WHERE k.trust_score > 5000 LIMIT 10").unwrap();
        match q {
            Query::Find(f) => {
                assert_eq!(f.pattern.nodes.len(), 1);
                assert_eq!(f.pattern.edges.len(), 0);
                assert!(f.where_clause.is_some());
                assert_eq!(f.limit, Some(10));
                assert_eq!(f.temporal, None);
                assert!(!f.history);
            },
            _ => panic!("Expected Find"),
        }
    }

    #[test]
    fn test_parse_find_history_edge_temporal_full() {
        let q = parse_query(
            "FIND HISTORY (a:KU)-[r:Extends]->(b:KU) WHERE a.trust_score > 1000 SCOPE LOCAL LIMIT 5 AT TIME 1719900000"
        ).unwrap();
        match q {
            Query::Find(f) => {
                assert!(f.history);
                assert_eq!(f.pattern.nodes.len(), 2);
                assert_eq!(f.pattern.edges.len(), 1);
                assert_eq!(f.pattern.edges[0].alias, Some("r".to_string()));
                assert!(f.where_clause.is_some());
                assert_eq!(f.scope, Scope::Local);
                assert_eq!(f.limit, Some(5));
                assert_eq!(f.temporal, Some(TemporalClause::AtTime(1719900000)));
            },
            _ => panic!("Expected Find"),
        }
    }
}
