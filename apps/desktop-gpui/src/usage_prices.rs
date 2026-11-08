//! The Custom model prices dialog's rules: `usagePriceForm.ts`, `usagePriceTable.ts` and
//! `usagePriceTargets.ts`.

use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// `UsageModelPriceOverride`; omitted cache rates use the input rate.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    #[serde(rename = "inputCostPerMillionTokens")]
    pub input: f64,
    #[serde(rename = "outputCostPerMillionTokens")]
    pub output: f64,
    #[serde(default, rename = "cacheReadCostPerMillionTokens")]
    pub cache_read: Option<f64>,
    #[serde(default, rename = "cacheWriteCostPerMillionTokens")]
    pub cache_write: Option<f64>,
}

impl Eq for Price {}

impl Price {
    pub fn to_json(self) -> Value {
        let mut map = Map::new();
        map.insert("inputCostPerMillionTokens".into(), json!(self.input));
        map.insert("outputCostPerMillionTokens".into(), json!(self.output));
        if let Some(value) = self.cache_read {
            map.insert("cacheReadCostPerMillionTokens".into(), json!(value));
        }
        if let Some(value) = self.cache_write {
            map.insert("cacheWriteCostPerMillionTokens".into(), json!(value));
        }
        Value::Object(map)
    }

    fn rate(&self, field: Field) -> Option<f64> {
        match field {
            Field::Input => Some(self.input),
            Field::Output => Some(self.output),
            Field::CacheRead => self.cache_read,
            Field::CacheWrite => self.cache_write,
        }
    }
}

/// `USAGE_PRICE_FIELDS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Input,
    Output,
    CacheRead,
    CacheWrite,
}

impl Field {
    pub const ALL: [Field; 4] = [
        Field::Input,
        Field::Output,
        Field::CacheRead,
        Field::CacheWrite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Output => "Output",
            Self::CacheRead => "Cache read",
            Self::CacheWrite => "Cache write",
        }
    }

    pub fn optional(self) -> bool {
        matches!(self, Self::CacheRead | Self::CacheWrite)
    }

    pub fn slot(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// `String(number)` as JavaScript prints it, so a saved rate shows as the web shows it.
pub fn js_number(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-6..1e21).contains(&magnitude) {
        let text = format!("{value:e}");
        return match text.split_once('e') {
            Some((mantissa, exponent)) if !exponent.starts_with('-') => {
                format!("{mantissa}e+{exponent}")
            }
            _ => text,
        };
    }
    format!("{value}")
}

/// `usagePriceForm(model, price)`'s four rate strings.
pub fn rates(price: Option<&Price>) -> [String; 4] {
    Field::ALL.map(|field| {
        price
            .and_then(|price| price.rate(field))
            .map(js_number)
            .unwrap_or_default()
    })
}

fn is_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_separator(character: char) -> bool {
    character == '.' || character == ','
}

/// `\d*[.,]?\d*`.
fn plain_number_prefix(text: &str) -> bool {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
    let rest = rest.strip_prefix(is_separator).unwrap_or(rest);
    is_digits(rest)
}

/// `\d+[.,]?\d*|[.,]\d+`.
fn mantissa(text: &str) -> bool {
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 {
        return plain_number_prefix(text);
    }
    text.strip_prefix(is_separator)
        .is_some_and(|rest| !rest.is_empty() && is_digits(rest))
}

fn is_edit_shape(text: &str) -> bool {
    let trimmed = text.trim();
    if plain_number_prefix(trimmed) {
        return true;
    }
    let Some(at) = trimmed.find(['e', 'E']) else {
        return false;
    };
    let (head, tail) = (&trimmed[..at], &trimmed[at + 1..]);
    let tail = tail.strip_prefix(['+', '-']).unwrap_or(tail);
    mantissa(head) && is_digits(tail)
}

/// `isUsagePriceEdit`: decimal and exponent edits and every deletion, including temporarily
/// invalid values; saving validates the result.
pub fn is_price_edit(previous: &str, next: &str) -> bool {
    if is_edit_shape(next) {
        return true;
    }
    let previous: Vec<u16> = previous.encode_utf16().collect();
    let next: Vec<u16> = next.encode_utf16().collect();
    if next.len() >= previous.len() {
        return false;
    }
    let mut start = 0;
    while start < next.len() && next[start] == previous[start] {
        start += 1;
    }
    next[start..] == previous[previous.len() - (next.len() - start)..]
}

/// `(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?`, anchored.
fn is_rate_shape(text: &str) -> bool {
    let (head, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    if let Some(exponent) = exponent {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if digits.is_empty() || !is_digits(digits) {
            return false;
        }
    }
    let digits = head.len() - head.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 {
        let rest = &head[digits..];
        return rest.is_empty() || rest.strip_prefix('.').is_some_and(is_digits);
    }
    head.strip_prefix('.')
        .is_some_and(|fraction| !fraction.is_empty() && is_digits(fraction))
}

/// A row's text as the four inputs hold it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    pub model: String,
    pub rates: [String; 4],
}

impl Form {
    pub fn new(model: &str, price: Option<&Price>) -> Self {
        Self {
            model: model.to_owned(),
            rates: rates(price),
        }
    }
}

/// `parseUsagePriceForm`: blank cache prices use the input rate, an explicit zero means free.
pub fn parse_form(form: &Form) -> Option<(String, Price)> {
    let model = form.model.trim();
    if model.is_empty() {
        return None;
    }
    let mut parsed: [Option<f64>; 4] = [None; 4];
    for field in Field::ALL {
        let raw = form.rates[field.slot()].trim().replacen(',', ".", 1);
        if raw.is_empty() {
            if field.optional() {
                continue;
            }
            return None;
        }
        if !is_rate_shape(&raw) {
            return None;
        }
        let value: f64 = raw.parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        parsed[field.slot()] = Some(value);
    }
    Some((
        model.to_owned(),
        Price {
            input: parsed[Field::Input.slot()]?,
            output: parsed[Field::Output.slot()]?,
            cache_read: parsed[Field::CacheRead.slot()],
            cache_write: parsed[Field::CacheWrite.slot()],
        },
    ))
}

/// One environment's price overrides and model mappings, as its settings hold them.
pub type Snapshot = (
    Option<BTreeMap<String, Price>>,
    Option<BTreeMap<String, String>>,
);

/// `UsagePriceTarget`: one environment the dialog applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub label: String,
    pub prices: Option<BTreeMap<String, Price>>,
    pub aliases: Option<BTreeMap<String, String>>,
    pub unavailable: Option<String>,
}

impl Target {
    fn price(&self, model: &str) -> Option<&Price> {
        self.prices.as_ref()?.get(model)
    }

    fn alias(&self, model: &str) -> Option<&str> {
        self.aliases.as_ref()?.get(model).map(String::as_str)
    }
}

/// The rate inputs a draft edited; a missing slot keeps the saved value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Values([Option<String>; 4]);

impl Values {
    pub fn get(&self, field: Field) -> Option<&str> {
        self.0[field.slot()].as_deref()
    }

    pub fn set(&mut self, field: Field, value: String) {
        self.0[field.slot()] = Some(value);
    }

    pub fn remove(&mut self, field: Field) {
        self.0[field.slot()] = None;
    }

    /// `Object.keys(values).length === 0`.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    fn all_blank(&self) -> bool {
        self.0.iter().flatten().all(|value| value.trim().is_empty())
    }

    fn any_filled(&self) -> bool {
        self.0.iter().flatten().any(|value| !value.is_empty())
    }
}

/// `UsagePriceDraft`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub id: String,
    pub model: String,
    pub is_new: bool,
    pub values: Values,
    /// Edited "Map to" model. Absent keeps each environment's mapping; blank removes it.
    pub alias: Option<String>,
    pub removed: bool,
}

impl Draft {
    pub fn new(id: String, model: &str, is_new: bool) -> Self {
        Self {
            id,
            model: model.to_owned(),
            is_new,
            values: Values::default(),
            alias: None,
            removed: false,
        }
    }

    /// Whether any error text would be shown: the model or a rate has text.
    pub fn has_text(&self) -> bool {
        !self.model.trim().is_empty() || self.values.any_filled()
    }
}

/// `isEmptyUsagePriceDraft`.
pub fn is_empty_draft(draft: &Draft) -> bool {
    draft.is_new
        && draft.model.trim().is_empty()
        && draft.alias.as_deref().unwrap_or("").trim().is_empty()
        && draft.values.all_blank()
}

/// What a cell shows when it is empty, and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub value: String,
    pub placeholder: &'static str,
}

impl Cell {
    /// The web's `matchesSaved` guard: a mixed or unavailable cell never matches.
    pub fn is_comparable(&self) -> bool {
        self.placeholder != "Mixed" && self.placeholder != "Unavailable"
    }
}

/// `usageAliasCell`.
pub fn alias_cell(targets: &[Target], model: &str) -> Cell {
    if targets.iter().any(|target| target.aliases.is_none()) {
        return Cell {
            value: String::new(),
            placeholder: "Unavailable",
        };
    }
    let values: Vec<&str> = targets
        .iter()
        .map(|target| target.alias(model).unwrap_or(""))
        .collect();
    if values.iter().any(|value| *value != values[0]) {
        return Cell {
            value: String::new(),
            placeholder: "Mixed",
        };
    }
    Cell {
        value: values.first().copied().unwrap_or("").to_owned(),
        placeholder: "None",
    }
}

/// `usagePriceCell`.
pub fn price_cell(targets: &[Target], model: &str, field: Field) -> Cell {
    let values: Vec<Option<String>> = targets
        .iter()
        .map(|target| {
            target
                .price(model)
                .map(|price| rates(Some(price))[field.slot()].clone())
        })
        .collect();
    if targets.iter().any(|target| target.prices.is_none()) {
        return Cell {
            value: String::new(),
            placeholder: "Unavailable",
        };
    }
    if values.iter().any(|value| *value != values[0]) {
        return Cell {
            value: String::new(),
            placeholder: "Mixed",
        };
    }
    let first = values.first();
    Cell {
        value: first.cloned().flatten().unwrap_or_default(),
        placeholder: if matches!(first, Some(None)) {
            "Automatic"
        } else if field.optional() {
            "Input rate"
        } else {
            "0.00"
        },
    }
}

/// `UsagePriceChange`; `None` removes the model's custom price or mapping.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Price(String, Option<Price>),
    Alias(String, Option<String>),
}

/// `usagePriceTableChanges`.
pub fn table_changes(target: &Target, drafts: &[Draft]) -> (Vec<Change>, HashMap<String, String>) {
    let mut changes = Vec::new();
    let mut errors: HashMap<String, String> = HashMap::new();
    for draft in drafts {
        if is_empty_draft(draft) {
            continue;
        }
        let model = draft.model.trim();
        let current_alias = target.alias(model);
        if draft.removed {
            if target.price(model).is_some() {
                changes.push(Change::Price(model.to_owned(), None));
            }
            if current_alias.is_some() {
                changes.push(Change::Alias(model.to_owned(), None));
            }
            continue;
        }
        let alias = draft
            .alias
            .as_deref()
            .or(current_alias)
            .unwrap_or("")
            .trim()
            .to_owned();
        if !alias.is_empty() {
            if draft.alias.is_none() && !draft.values.is_empty() {
                errors.insert(
                    draft.id.clone(),
                    format!(
                        "{} maps this model. Remove it from \"Apply to\" or clear its mapping first.",
                        target.label
                    ),
                );
                continue;
            }
            if model.is_empty() {
                errors.insert(draft.id.clone(), "Enter a model ID.".to_owned());
            } else if alias == model {
                errors.insert(draft.id.clone(), "Map to a different model.".to_owned());
            } else {
                if Some(alias.as_str()) != current_alias {
                    changes.push(Change::Alias(model.to_owned(), Some(alias)));
                }
                if target.price(model).is_some() {
                    changes.push(Change::Price(model.to_owned(), None));
                }
            }
            continue;
        }
        if current_alias.is_some() {
            changes.push(Change::Alias(model.to_owned(), None));
            if target.price(model).is_none() && draft.values.all_blank() {
                continue;
            }
        }
        let original = Form::new(model, target.price(model));
        let mut form = original.clone();
        for field in Field::ALL {
            if let Some(value) = draft.values.get(field) {
                form.rates[field.slot()] = value.to_owned();
            }
        }
        match parse_form(&form) {
            None => {
                let missing = Field::ALL
                    .into_iter()
                    .find(|field| !field.optional() && form.rates[field.slot()].trim().is_empty());
                let message = if model.is_empty() {
                    "Enter a model ID.".to_owned()
                } else if let Some(field) = missing {
                    format!("{} is required on {}.", field.label(), target.label)
                } else {
                    "Use non-negative numbers for prices.".to_owned()
                };
                errors.insert(draft.id.clone(), message);
            }
            Some((parsed_model, price)) => {
                let next = Form::new(model, Some(&price));
                if original.rates != next.rates {
                    changes.push(Change::Price(parsed_model, Some(price)));
                }
            }
        }
    }
    // Check the final batch so clearing a mapping and reversing it in one save stays valid.
    let mut aliases: HashMap<String, String> = target
        .aliases
        .iter()
        .flatten()
        .map(|(model, alias)| (model.clone(), alias.clone()))
        .collect();
    for change in &changes {
        if let Change::Alias(model, alias) = change {
            match alias {
                None => {
                    aliases.remove(model);
                }
                Some(alias) => {
                    aliases.insert(model.clone(), alias.clone());
                }
            }
        }
    }
    for draft in drafts {
        if draft.removed || errors.contains_key(&draft.id) {
            continue;
        }
        let mut model = draft.model.trim().to_owned();
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(next) = aliases.get(&model) {
            if !seen.insert(model.clone()) {
                errors.insert(
                    draft.id.clone(),
                    format!("Model mappings cannot form a cycle on {}.", target.label),
                );
                break;
            }
            model = next.clone();
        }
    }
    (changes, errors)
}

/// `usagePriceTableErrors`: the errors of every environment that can take the changes.
pub fn table_errors(targets: &[Target], drafts: &[Draft]) -> HashMap<String, String> {
    let mut errors = HashMap::new();
    for target in targets
        .iter()
        .filter(|target| target.unavailable.is_none() && target.prices.is_some())
    {
        errors.extend(table_changes(target, drafts).1);
    }
    errors
}

/// What `writeUsagePrices` does for one environment.
#[derive(Clone, Debug, PartialEq)]
pub enum Write {
    Failed(String),
    /// Nothing to change on this environment.
    Saved,
    Patch(Value),
}

/// The `server.updateSettings` patch of one environment's changes, or why it cannot be sent.
pub fn plan_write(target: &Target, changes: &[Change]) -> Write {
    if let Some(reason) = &target.unavailable {
        return Write::Failed(reason.clone());
    }
    let has_alias = changes
        .iter()
        .any(|change| matches!(change, Change::Alias(..)));
    if has_alias && target.aliases.is_none() {
        return Write::Failed("Update server to map models".to_owned());
    }
    if changes.is_empty() {
        return Write::Saved;
    }
    let mut prices = Map::new();
    let mut aliases = Map::new();
    for change in changes {
        match change {
            Change::Price(model, price) => {
                prices.insert(model.clone(), price.map_or(Value::Null, Price::to_json));
            }
            Change::Alias(model, alias) => {
                aliases.insert(
                    model.clone(),
                    alias.clone().map_or(Value::Null, Value::from),
                );
            }
        }
    }
    let mut patch = Map::new();
    if !prices.is_empty() {
        patch.insert("usagePriceOverrides".into(), Value::Object(prices));
    }
    if !aliases.is_empty() {
        patch.insert("usageModelAliases".into(), Value::Object(aliases));
    }
    Write::Patch(json!({ "patch": Value::Object(patch) }))
}

/// The server's settings after a successful patch: the same merge its reducer applies, so the
/// dialog shows the saved rows without waiting for a settings read.
pub fn apply_changes(target: &mut Target, changes: &[Change]) {
    for change in changes {
        match change {
            Change::Price(model, price) => {
                if let Some(prices) = target.prices.as_mut() {
                    match price {
                        Some(price) => {
                            prices.insert(model.clone(), *price);
                        }
                        None => {
                            prices.remove(model);
                        }
                    }
                }
            }
            Change::Alias(model, alias) => {
                if let Some(aliases) = target.aliases.as_mut() {
                    match alias {
                        Some(alias) => {
                            aliases.insert(model.clone(), alias.clone());
                        }
                        None => {
                            aliases.remove(model);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(input: f64, output: f64) -> Price {
        Price {
            input,
            output,
            cache_read: None,
            cache_write: None,
        }
    }

    fn target(prices: &[(&str, Price)], aliases: &[(&str, &str)]) -> Target {
        Target {
            label: "Local".to_owned(),
            prices: Some(
                prices
                    .iter()
                    .map(|(model, price)| ((*model).to_owned(), *price))
                    .collect(),
            ),
            aliases: Some(
                aliases
                    .iter()
                    .map(|(model, alias)| ((*model).to_owned(), (*alias).to_owned()))
                    .collect(),
            ),
            unavailable: None,
        }
    }

    fn draft(id: &str, model: &str, is_new: bool, values: &[(Field, &str)]) -> Draft {
        let mut draft = Draft::new(id.to_owned(), model, is_new);
        for (field, value) in values {
            draft.values.set(*field, (*value).to_owned());
        }
        draft
    }

    #[test]
    fn rates_print_like_javascript() {
        assert_eq!(js_number(3.0), "3");
        assert_eq!(js_number(0.5), "0.5");
        assert_eq!(js_number(1e21), "1e+21");
        assert_eq!(js_number(1e-7), "1e-7");
        assert_eq!(js_number(0.0), "0");
    }

    #[test]
    fn edits_allow_partial_numbers_and_every_deletion() {
        for ok in [
            "", "1", "1.", ".5", "1,5", " 2 ", "1e", "1e-", "1.5e+3", ".5e2",
        ] {
            assert!(is_price_edit("x", ok), "{ok:?}");
        }
        for bad in ["abc", "1..2", "-1", "e5", "1e5e", "1 2"] {
            assert!(!is_price_edit("1", bad), "{bad:?}");
        }
        // A deletion from a value that is not a number is still an edit.
        assert!(is_price_edit("1e5x", "1ex"));
        assert!(is_price_edit("1a2", "1a"));
        assert!(!is_price_edit("12", "1a2"));
        assert!(!is_price_edit("1a", "1b"));
    }

    #[test]
    fn forms_parse_like_the_web() {
        let form = |model: &str, rates: [&str; 4]| Form {
            model: model.to_owned(),
            rates: rates.map(str::to_owned),
        };
        assert_eq!(
            parse_form(&form(" m ", ["1,5", "2e1", "", "0"])),
            Some((
                "m".to_owned(),
                Price {
                    input: 1.5,
                    output: 20.0,
                    cache_read: None,
                    cache_write: Some(0.0),
                }
            ))
        );
        assert_eq!(parse_form(&form("", ["1", "1", "", ""])), None);
        assert_eq!(parse_form(&form("m", ["1", "", "", ""])), None);
        assert_eq!(parse_form(&form("m", ["1", "-1", "", ""])), None);
        assert_eq!(parse_form(&form("m", ["1", "1e", "", ""])), None);
        assert_eq!(parse_form(&form("m", ["1", "1e999", "", ""])), None);
        assert!(parse_form(&form("m", [".5", "5.", "", ""])).is_some());
    }

    #[test]
    fn cells_show_mixed_and_unavailable() {
        let a = target(&[("m", price(1.0, 2.0))], &[]);
        let b = target(&[("m", price(1.0, 3.0))], &[("m", "n")]);
        let both = [a.clone(), b.clone()];
        assert_eq!(price_cell(&both, "m", Field::Input).value, "1");
        assert_eq!(price_cell(&both, "m", Field::Output).placeholder, "Mixed");
        assert_eq!(alias_cell(&both, "m").placeholder, "Mixed");
        assert_eq!(
            price_cell(std::slice::from_ref(&a), "x", Field::Input).placeholder,
            "Automatic"
        );
        assert_eq!(
            price_cell(std::slice::from_ref(&a), "m", Field::CacheRead).placeholder,
            "Input rate"
        );
        assert_eq!(
            alias_cell(std::slice::from_ref(&a), "m").placeholder,
            "None"
        );
        let mut missing = b;
        missing.prices = None;
        missing.aliases = None;
        assert_eq!(
            price_cell(&[a.clone(), missing.clone()], "m", Field::Input).placeholder,
            "Unavailable"
        );
        assert_eq!(alias_cell(&[a, missing], "m").placeholder, "Unavailable");
        assert!(
            !Cell {
                value: String::new(),
                placeholder: "Mixed"
            }
            .is_comparable()
        );
    }

    #[test]
    fn changes_cover_edits_resets_and_mappings() {
        let t = target(
            &[("old", price(1.0, 2.0)), ("both", price(1.0, 1.0))],
            &[("map", "old")],
        );
        let (changes, errors) = table_changes(
            &t,
            &[
                draft(
                    "new:0",
                    "fresh",
                    true,
                    &[(Field::Input, "3"), (Field::Output, "4")],
                ),
                draft("model:old", "old", false, &[(Field::Output, "5")]),
                {
                    let mut removed = Draft::new("model:both".into(), "both", false);
                    removed.removed = true;
                    removed
                },
                {
                    let mut mapped = Draft::new("new:1".into(), "alias", true);
                    mapped.alias = Some("old".into());
                    mapped
                },
            ],
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            changes,
            vec![
                Change::Price("fresh".into(), Some(price(3.0, 4.0))),
                Change::Price("old".into(), Some(price(1.0, 5.0))),
                Change::Price("both".into(), None),
                Change::Alias("alias".into(), Some("old".into())),
            ]
        );
    }

    #[test]
    fn unchanged_rows_and_empty_drafts_change_nothing() {
        let t = target(&[("old", price(1.0, 2.0))], &[]);
        let (changes, errors) = table_changes(
            &t,
            &[
                draft("new:0", "", true, &[]),
                draft("model:old", "old", false, &[(Field::Input, "1.0")]),
            ],
        );
        assert!(changes.is_empty() && errors.is_empty());
    }

    #[test]
    fn errors_use_the_webs_wording() {
        let t = target(&[("old", price(1.0, 2.0))], &[("map", "old")]);
        let errors = |drafts: &[Draft]| table_changes(&t, drafts).1;
        let message = |drafts: &[Draft], id: &str| errors(drafts).remove(id).unwrap_or_default();
        assert_eq!(
            message(&[draft("a", "", true, &[(Field::Input, "1")])], "a"),
            "Enter a model ID."
        );
        assert_eq!(
            message(&[draft("a", "x", true, &[(Field::Input, "1")])], "a"),
            "Output is required on Local."
        );
        assert_eq!(
            message(
                &[draft(
                    "a",
                    "x",
                    true,
                    &[(Field::Input, "1"), (Field::Output, "-")]
                )],
                "a"
            ),
            "Use non-negative numbers for prices."
        );
        assert_eq!(
            message(
                &[draft("model:map", "map", false, &[(Field::Input, "1")])],
                "model:map"
            ),
            "Local maps this model. Remove it from \"Apply to\" or clear its mapping first."
        );
        let mut same = Draft::new("a".into(), "x", true);
        same.alias = Some("x".into());
        assert_eq!(message(&[same], "a"), "Map to a different model.");
        let mut cycle = Draft::new("a".into(), "old", true);
        cycle.alias = Some("map".into());
        assert_eq!(
            message(&[cycle], "a"),
            "Model mappings cannot form a cycle on Local."
        );
    }

    #[test]
    fn clearing_a_mapping_and_reversing_it_in_one_save_is_valid() {
        let t = target(&[], &[("a", "b")]);
        let mut clear = Draft::new("model:a".into(), "a", false);
        clear.alias = Some(String::new());
        let mut reverse = Draft::new("new:0".into(), "b", true);
        reverse.alias = Some("a".into());
        let (changes, errors) = table_changes(&t, &[clear, reverse]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            changes,
            vec![
                Change::Alias("a".into(), None),
                Change::Alias("b".into(), Some("a".into())),
            ]
        );
    }

    #[test]
    fn errors_skip_unavailable_environments() {
        let good = target(&[], &[]);
        let mut offline = target(&[], &[]);
        offline.unavailable = Some("Offline".into());
        let drafts = [draft("a", "", true, &[(Field::Input, "1")])];
        assert_eq!(table_errors(std::slice::from_ref(&good), &drafts).len(), 1);
        assert!(table_errors(&[offline], &drafts).is_empty());
    }

    #[test]
    fn writes_build_one_patch_per_environment() {
        let t = target(&[("old", price(1.0, 2.0))], &[]);
        let changes = [
            Change::Price("old".into(), None),
            Change::Price(
                "new".into(),
                Some(Price {
                    cache_read: Some(0.0),
                    ..price(1.0, 2.0)
                }),
            ),
            Change::Alias("x".into(), Some("y".into())),
            Change::Alias("z".into(), None),
        ];
        assert_eq!(
            plan_write(&t, &changes),
            Write::Patch(json!({"patch": {
                "usagePriceOverrides": {
                    "old": null,
                    "new": {
                        "inputCostPerMillionTokens": 1.0,
                        "outputCostPerMillionTokens": 2.0,
                        "cacheReadCostPerMillionTokens": 0.0,
                    },
                },
                "usageModelAliases": {"x": "y", "z": null},
            }}))
        );
        assert_eq!(plan_write(&t, &[]), Write::Saved);
        let mut old_server = t.clone();
        old_server.aliases = None;
        assert_eq!(
            plan_write(&old_server, &changes),
            Write::Failed("Update server to map models".into())
        );
        let mut offline = t;
        offline.unavailable = Some("Offline".into());
        assert_eq!(
            plan_write(&offline, &changes),
            Write::Failed("Offline".into())
        );
    }

    #[test]
    fn saved_changes_update_the_local_settings() {
        let mut t = target(&[("old", price(1.0, 2.0))], &[("a", "b")]);
        apply_changes(
            &mut t,
            &[
                Change::Price("old".into(), None),
                Change::Price("new".into(), Some(price(3.0, 4.0))),
                Change::Alias("a".into(), None),
                Change::Alias("c".into(), Some("d".into())),
            ],
        );
        assert_eq!(t.prices.as_ref().map(BTreeMap::len), Some(1));
        assert_eq!(t.price("new"), Some(&price(3.0, 4.0)));
        assert_eq!(t.alias("a"), None);
        assert_eq!(t.alias("c"), Some("d"));
    }
}
