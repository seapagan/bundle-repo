use super::SecretScanError;
use secrets_scanner::{ScanConfig, Scanner};
use std::collections::{BTreeMap, BTreeSet};
use toml::{Table, Value};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum RulePhase {
    PathOnly,
    Keyworded,
    Unkeyworded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RuleOrder {
    pub(super) phase: RulePhase,
    pub(super) source_index: usize,
    pub(super) owner_partition: usize,
}

pub(super) struct RulePartition {
    pub(super) ordinal: usize,
    pub(super) scanner: Scanner,
    #[cfg(test)]
    pub(super) serialized_ruleset: String,
}

pub(super) struct PartitionedScanners {
    pub(super) partitions: Vec<RulePartition>,
    pub(super) rule_order: BTreeMap<String, RuleOrder>,
}

struct SourceRule {
    id: String,
    value: Value,
    order: RuleOrder,
}

pub(super) fn build_partitioned_scanners(
    source: &str,
    requested_partitions: usize,
    config: &ScanConfig,
) -> Result<PartitionedScanners, SecretScanError> {
    let (document, source_rules, partition_count) =
        prepare_source_rules(source, requested_partitions)?;
    let (partitions, compiled_union) =
        build_partitions(&document, &source_rules, partition_count, config)?;
    let rule_order = source_rules
        .into_iter()
        .map(|rule| (rule.id, rule.order))
        .collect::<BTreeMap<_, _>>();
    validate_partition_integrity(
        &partitions,
        &rule_order,
        &compiled_union,
        partition_count,
    )?;

    Ok(PartitionedScanners {
        partitions,
        rule_order,
    })
}

fn prepare_source_rules(
    source: &str,
    requested_partitions: usize,
) -> Result<(Table, Vec<SourceRule>, usize), SecretScanError> {
    let mut document = toml::from_str::<Table>(source)
        .map_err(|_| SecretScanError::InvalidRuleset)?;
    harden_path_allowlists(&mut document)?;
    let rules = document
        .remove("rules")
        .and_then(|value| value.as_array().cloned())
        .filter(|rules| !rules.is_empty())
        .ok_or(SecretScanError::InvalidRuleset)?;
    let partition_count = requested_partitions.max(1).min(rules.len());
    let source_rules = parse_source_rules(rules, partition_count)?;

    Ok((document, source_rules, partition_count))
}

fn build_partitions(
    document: &Table,
    source_rules: &[SourceRule],
    partition_count: usize,
    config: &ScanConfig,
) -> Result<(Vec<RulePartition>, BTreeSet<String>), SecretScanError> {
    let mut assigned = vec![Vec::new(); partition_count];
    for source_rule in source_rules {
        assigned[source_rule.order.owner_partition]
            .push(source_rule.value.clone());
    }

    let mut partitions = Vec::with_capacity(partition_count);
    let mut compiled_union = BTreeSet::new();
    for (ordinal, assigned_rules) in assigned.into_iter().enumerate() {
        let (scanner, _serialized_ruleset) = build_partition(
            document,
            &assigned_rules,
            config,
            &mut compiled_union,
        )?;
        partitions.push(RulePartition {
            ordinal,
            scanner,
            #[cfg(test)]
            serialized_ruleset: _serialized_ruleset,
        });
    }

    Ok((partitions, compiled_union))
}

fn validate_partition_integrity(
    partitions: &[RulePartition],
    rule_order: &BTreeMap<String, RuleOrder>,
    compiled_union: &BTreeSet<String>,
    partition_count: usize,
) -> Result<(), SecretScanError> {
    let source_ids = rule_order.keys().cloned().collect::<BTreeSet<_>>();
    if *compiled_union != source_ids
        || compiled_union.len() != rule_order.len()
        || partitions.len() != partition_count
    {
        return Err(SecretScanError::PartitionIntegrity);
    }
    Ok(())
}

fn harden_path_allowlists(
    document: &mut Table,
) -> Result<(), SecretScanError> {
    harden_optional_allowlist(document, "allowlist")?;
    harden_allowlist_array(document, "allowlists")?;

    let rules = document
        .get_mut("rules")
        .and_then(Value::as_array_mut)
        .ok_or(SecretScanError::InvalidRuleset)?;
    for rule in rules {
        let table =
            rule.as_table_mut().ok_or(SecretScanError::InvalidRuleset)?;
        harden_allowlist_array(table, "allowlists")?;
    }
    Ok(())
}

fn harden_optional_allowlist(
    parent: &mut Table,
    key: &str,
) -> Result<(), SecretScanError> {
    let Some(value) = parent.get_mut(key) else {
        return Ok(());
    };
    let keep = harden_allowlist(
        value
            .as_table_mut()
            .ok_or(SecretScanError::InvalidRuleset)?,
    )?;
    if !keep {
        parent.remove(key);
    }
    Ok(())
}

fn harden_allowlist_array(
    parent: &mut Table,
    key: &str,
) -> Result<(), SecretScanError> {
    let Some(value) = parent.get_mut(key) else {
        return Ok(());
    };
    let allowlists = value
        .as_array_mut()
        .ok_or(SecretScanError::InvalidRuleset)?;
    let mut hardened = Vec::with_capacity(allowlists.len());
    for mut allowlist in std::mem::take(allowlists) {
        let keep = harden_allowlist(
            allowlist
                .as_table_mut()
                .ok_or(SecretScanError::InvalidRuleset)?,
        )?;
        if keep {
            hardened.push(allowlist);
        }
    }
    *allowlists = hardened;
    Ok(())
}

fn harden_allowlist(allowlist: &mut Table) -> Result<bool, SecretScanError> {
    let has_paths = match allowlist.get("paths") {
        None => false,
        Some(Value::Array(paths)) => {
            for path in paths {
                path.as_str().ok_or(SecretScanError::InvalidRuleset)?;
            }
            !paths.is_empty()
        }
        Some(_) => return Err(SecretScanError::InvalidRuleset),
    };
    if !has_paths {
        return Ok(true);
    }

    if allowlist_condition_is_and(allowlist)? {
        return Ok(false);
    }
    allowlist.remove("paths");
    has_non_path_criteria(allowlist)
}

fn allowlist_condition_is_and(
    allowlist: &Table,
) -> Result<bool, SecretScanError> {
    match allowlist.get("condition") {
        None => Ok(false),
        Some(Value::String(condition)) => {
            if condition.eq_ignore_ascii_case("or") {
                Ok(false)
            } else if condition.eq_ignore_ascii_case("and") {
                Ok(true)
            } else {
                Err(SecretScanError::InvalidRuleset)
            }
        }
        Some(_) => Err(SecretScanError::InvalidRuleset),
    }
}

fn has_non_path_criteria(allowlist: &Table) -> Result<bool, SecretScanError> {
    for key in ["regexes", "stopwords"] {
        match allowlist.get(key) {
            None => {}
            Some(Value::Array(values)) => {
                for value in values {
                    value.as_str().ok_or(SecretScanError::InvalidRuleset)?;
                }
                if !values.is_empty() {
                    return Ok(true);
                }
            }
            Some(_) => return Err(SecretScanError::InvalidRuleset),
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn hardened_ruleset_for_tests(
    source: &str,
) -> Result<Table, SecretScanError> {
    let mut document = toml::from_str::<Table>(source)
        .map_err(|_| SecretScanError::InvalidRuleset)?;
    harden_path_allowlists(&mut document)?;
    Ok(document)
}

fn parse_source_rules(
    rules: Vec<Value>,
    partition_count: usize,
) -> Result<Vec<SourceRule>, SecretScanError> {
    let mut ids = BTreeSet::new();
    let mut source_rules = Vec::with_capacity(rules.len());
    for (source_index, value) in rules.into_iter().enumerate() {
        let table = value.as_table().ok_or(SecretScanError::InvalidRuleset)?;
        let id = table
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or(SecretScanError::InvalidRuleset)?
            .to_string();
        if !ids.insert(id.clone()) {
            return Err(SecretScanError::InvalidRuleset);
        }
        let phase = rule_phase(table)?;
        source_rules.push(SourceRule {
            id,
            value,
            order: RuleOrder {
                phase,
                source_index,
                owner_partition: source_index % partition_count,
            },
        });
    }
    Ok(source_rules)
}

fn rule_phase(table: &Table) -> Result<RulePhase, SecretScanError> {
    let regex = optional_string(table, "regex")?;
    let path = optional_string(table, "path")?;
    if regex.is_none() && path.is_none() {
        return Err(SecretScanError::InvalidRuleset);
    }
    let keywords = match table.get("keywords") {
        None => false,
        Some(Value::Array(values)) => {
            let mut non_empty = false;
            for value in values {
                let keyword = value
                    .as_str()
                    .filter(|keyword| !keyword.is_empty())
                    .ok_or(SecretScanError::InvalidRuleset)?;
                non_empty |= !keyword.is_empty();
            }
            non_empty
        }
        Some(_) => return Err(SecretScanError::InvalidRuleset),
    };
    Ok(if regex.is_none() {
        RulePhase::PathOnly
    } else if keywords {
        RulePhase::Keyworded
    } else {
        RulePhase::Unkeyworded
    })
}

fn optional_string<'a>(
    table: &'a Table,
    key: &str,
) -> Result<Option<&'a str>, SecretScanError> {
    match table.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(_) => Err(SecretScanError::InvalidRuleset),
    }
}

fn build_partition(
    base: &Table,
    assigned_rules: &[Value],
    config: &ScanConfig,
    compiled_union: &mut BTreeSet<String>,
) -> Result<(Scanner, String), SecretScanError> {
    if assigned_rules.is_empty() {
        return Err(SecretScanError::PartitionIntegrity);
    }
    let mut subset = base.clone();
    subset.insert("rules".to_string(), Value::Array(assigned_rules.to_vec()));
    let serialized = toml::to_string(&subset)
        .map_err(|_| SecretScanError::InvalidRuleset)?;
    prove_round_trip(base, assigned_rules, &serialized)?;

    let scanner = Scanner::from_toml(&serialized)
        .map_err(SecretScanError::PartitionSetup)?
        .with_config(config.clone());
    let assigned_ids = assigned_rules
        .iter()
        .map(rule_id)
        .collect::<Result<BTreeSet<_>, _>>()?;
    let compiled_ids = scanner
        .engine()
        .rules()
        .into_iter()
        .map(|rule| rule.id.clone())
        .collect::<BTreeSet<_>>();
    prove_compiled_partition(
        &assigned_ids,
        &compiled_ids,
        scanner.engine().rule_count(),
        assigned_rules.len(),
        compiled_union,
    )?;
    Ok((scanner, serialized))
}

fn prove_compiled_partition(
    assigned_ids: &BTreeSet<String>,
    compiled_ids: &BTreeSet<String>,
    compiled_count: usize,
    assigned_count: usize,
    compiled_union: &mut BTreeSet<String>,
) -> Result<(), SecretScanError> {
    if compiled_count != assigned_count
        || assigned_ids.len() != assigned_count
        || compiled_ids.len() != compiled_count
        || compiled_ids != assigned_ids
        || !compiled_union.is_disjoint(compiled_ids)
    {
        return Err(SecretScanError::PartitionIntegrity);
    }
    compiled_union.extend(compiled_ids.iter().cloned());
    Ok(())
}

#[cfg(test)]
pub(super) fn prove_compiled_partition_for_tests(
    assigned_ids: &[&str],
    compiled_ids: &[&str],
    compiled_count: usize,
    compiled_union: &mut BTreeSet<String>,
) -> Result<(), SecretScanError> {
    prove_compiled_partition(
        &assigned_ids.iter().map(|id| (*id).to_string()).collect(),
        &compiled_ids.iter().map(|id| (*id).to_string()).collect(),
        compiled_count,
        assigned_ids.len(),
        compiled_union,
    )
}

fn prove_round_trip(
    base: &Table,
    assigned_rules: &[Value],
    serialized: &str,
) -> Result<(), SecretScanError> {
    let mut parsed = toml::from_str::<Table>(serialized)
        .map_err(|_| SecretScanError::InvalidRuleset)?;
    let parsed_rules = parsed
        .remove("rules")
        .ok_or(SecretScanError::PartitionIntegrity)?;
    if parsed != *base || parsed_rules != Value::Array(assigned_rules.to_vec())
    {
        return Err(SecretScanError::PartitionIntegrity);
    }
    Ok(())
}

fn rule_id(value: &Value) -> Result<String, SecretScanError> {
    value
        .as_table()
        .and_then(|table| table.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(SecretScanError::PartitionIntegrity)
}
