//! `--select` / `--tag` 选择逻辑（纯集合运算）
//!
//! 语义：同一个 key 的多个 value 之间为 OR；不同 key 之间为 AND。

use std::collections::{BTreeMap, BTreeSet};

use super::tags::TagSet;

/// 汇总每个实体命中了哪些 key，再按“必须命中全部 key”过滤
pub fn filter_entities<T, F>(entities: Vec<T>, required: &TagSet, mut matched_keys: F) -> Vec<T>
where
    F: FnMut(&T) -> BTreeSet<String>,
{
    let required_keys: BTreeSet<&String> = required.keys().collect();
    entities
        .into_iter()
        .filter(|entity| {
            let keys = matched_keys(entity);
            required_keys.iter().all(|k| keys.contains(*k))
        })
        .collect()
}

/// 将 (实体id, tag_key) 命中对汇总为 实体 -> 命中 key 集合
pub fn group_matched_keys<I, S>(hits: I) -> BTreeMap<String, BTreeSet<String>>
where
    I: IntoIterator<Item = (S, S)>,
    S: Into<String>,
{
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (entity_id, key) in hits {
        map.entry(entity_id.into()).or_default().insert(key.into());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn and_across_keys_or_within_key() {
        // 实体 A: License=MIT; 实体 B: License=MIT+Language=Java
        let required: TagSet = BTreeMap::from([
            ("License".to_owned(), BTreeSet::from(["MIT".to_owned()])),
            (
                "Language".to_owned(),
                BTreeSet::from(["Java".to_owned(), "JavaScript".to_owned()]),
            ),
        ]);
        let hits = vec![("A", "License"), ("B", "License"), ("B", "Language")];
        let grouped = group_matched_keys(hits);
        let entities = vec!["A", "B"];
        let picked = filter_entities(entities, &required, |id| {
            grouped.get(*id).cloned().unwrap_or_default()
        });
        assert_eq!(picked, vec!["B"]);
    }
}
