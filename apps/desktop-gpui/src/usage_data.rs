//! Pure logic behind the Usage dialog: decoding `server.getUsageSummary`, merging it, formatting
//! and chart math. Each part ports the web module named beside it.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::shell::timestamp_ms;

/// `USAGE_CONTRACT_VERSION` and `USAGE_MERGE_COMPATIBLE_SINCE`, packages/contracts/src/usage.ts.
pub const CONTRACT_VERSION: u64 = 6;
const MERGE_COMPATIBLE_SINCE: u64 = 4;

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    Codex,
    Claude,
    Grok,
    Cursor,
    OpenCode,
    Antigravity,
    Pi,
    Muse,
}

/// How a provider color resolves against the theme (usageProviders.ts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    Fixed(u32),
    Contrast,
    ContrastSoft,
}

impl Provider {
    /// PROVIDER_ORDER, apps/web/src/components/usage/usageProviders.ts.
    pub const ORDER: [Provider; 8] = [
        Provider::Codex,
        Provider::Claude,
        Provider::Grok,
        Provider::Cursor,
        Provider::OpenCode,
        Provider::Antigravity,
        Provider::Pi,
        Provider::Muse,
    ];

    pub fn slot(self) -> usize {
        Self::ORDER.iter().position(|p| *p == self).unwrap_or(0)
    }

    fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "codex" => Self::Codex,
            "claude" => Self::Claude,
            "grok" => Self::Grok,
            "cursor" => Self::Cursor,
            "opencode" => Self::OpenCode,
            "antigravity" => Self::Antigravity,
            "pi" => Self::Pi,
            "muse" => Self::Muse,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Grok => "Grok Build",
            Self::Cursor => "Cursor",
            Self::OpenCode => "OpenCode",
            Self::Antigravity => "Antigravity",
            Self::Pi => "Pi",
            Self::Muse => "Muse Code",
        }
    }

    pub fn driver_kind(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claudeAgent",
            Self::Grok => "grok",
            Self::Cursor => "cursor",
            Self::OpenCode => "opencode",
            Self::Antigravity => "antigravity",
            Self::Pi => "pi",
            Self::Muse => "muse",
        }
    }

    pub fn tint(self) -> Tint {
        match self {
            Self::Codex => Tint::Contrast,
            Self::Claude => Tint::Fixed(0xd97757),
            Self::Grok => Tint::ContrastSoft,
            Self::Cursor => Tint::Fixed(0x8b8b8b),
            Self::OpenCode => Tint::Fixed(0x5b9bbd),
            Self::Antigravity => Tint::Fixed(0x8c7bd1),
            Self::Pi => Tint::Fixed(0x4d9f8a),
            Self::Muse => Tint::Fixed(0x0866ff),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tokens {
    pub uncached: f64,
    pub cached: f64,
    pub cache_creation: f64,
    pub output: f64,
    pub reasoning: f64,
}

impl Tokens {
    /// Reasoning is a subset of output and is not added again (usageMerge.ts bucketTokens).
    pub fn total(&self) -> f64 {
        self.uncached + self.cached + self.cache_creation + self.output
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bucket {
    pub day: String,
    pub hour_start: Option<String>,
    pub provider: Provider,
    pub model: String,
    pub source_path: Option<String>,
    pub totals: Tokens,
    pub cost_usd: f64,
    pub cache_savings_usd: f64,
    /// input, cache read, cache write, output
    pub category_cost: Option<[f64; 4]>,
    pub fast_cost: f64,
    pub ultrafast_cost: f64,
    pub speed_premium: f64,
    pub records: f64,
    pub unpriced_records: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    Ok,
    Missing,
    Partial,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub host_id: String,
    pub provider: Provider,
    pub home: String,
    pub volume_id: String,
    pub status: SourceStatus,
    pub malformed: f64,
    pub distinct_sessions: f64,
    pub message: Option<String>,
    pub has_action: bool,
    pub refreshing: bool,
}

impl Source {
    fn key(&self) -> [&str; 4] {
        [
            &self.host_id,
            self.provider.driver_kind(),
            &self.home,
            &self.volume_id,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub contract_version: u64,
    pub time_zone: String,
    pub buckets: Vec<Bucket>,
    pub sources: Vec<Source>,
}

fn text(value: &Value, key: &str) -> Option<String> {
    let trimmed = value.get(key)?.as_str()?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn int(value: &Value, key: &str) -> Option<f64> {
    let n = value.get(key)?.as_f64()?;
    (n >= 0.0 && n.fract() == 0.0).then_some(n)
}

fn num(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64()
}

fn is_day(day: &str) -> bool {
    let b = day.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn decode_bucket(value: &Value) -> Option<Bucket> {
    let day = text(value, "day").filter(|d| is_day(d))?;
    let totals = value.get("totals")?;
    let category_cost = match value.get("categoryCostUsd") {
        None => None,
        Some(c) => Some([
            num(c, "input")?,
            num(c, "cacheRead")?,
            num(c, "cacheWrite")?,
            num(c, "output")?,
        ]),
    };
    let optional = |key| match value.get(key) {
        None => Some(0.0),
        Some(v) => v.as_f64(),
    };
    let cost_source = value.get("costSource")?.as_str()?;
    if !matches!(cost_source, "providerReported" | "modelPriced" | "unpriced") {
        return None;
    }
    int(value, "sessions")?;
    Some(Bucket {
        day,
        hour_start: match value.get("hourStart") {
            None => None,
            Some(_) => Some(text(value, "hourStart")?),
        },
        provider: Provider::from_wire(value.get("provider")?.as_str()?)?,
        model: text(value, "model")?,
        source_path: match value.get("sourcePath") {
            None => None,
            Some(_) => Some(text(value, "sourcePath")?),
        },
        totals: Tokens {
            uncached: int(totals, "uncachedInputTokens")?,
            cached: int(totals, "cachedInputTokens")?,
            cache_creation: int(totals, "cacheCreationTokens")?,
            output: int(totals, "outputTokens")?,
            reasoning: int(totals, "reasoningTokens")?,
        },
        cost_usd: num(value, "costUsd")?,
        cache_savings_usd: num(value, "cacheSavingsUsd")?,
        category_cost,
        fast_cost: optional("fastCostUsd")?,
        ultrafast_cost: optional("ultrafastCostUsd")?,
        speed_premium: optional("speedPremiumUsd")?,
        records: int(value, "records")?,
        unpriced_records: int(value, "unpricedRecords")?,
    })
}

fn decode_source(value: &Value) -> Option<Source> {
    let fingerprint = value.get("fingerprint")?;
    let status = match value.get("status")?.as_str()? {
        "ok" => SourceStatus::Ok,
        "missing" => SourceStatus::Missing,
        "partial" => SourceStatus::Partial,
        "failed" => SourceStatus::Failed,
        _ => return None,
    };
    int(value, "scannedFiles")?;
    int(value, "skippedFiles")?;
    let message = match value.get("message")? {
        Value::Null => None,
        _ => Some(text(value, "message")?),
    };
    let has_action = match value.get("action") {
        None => false,
        Some(a) if a == "enableCursorKeychain" => true,
        Some(_) => return None,
    };
    let refreshing = match value.get("refreshing") {
        None => false,
        Some(Value::Bool(true)) => true,
        Some(_) => return None,
    };
    Some(Source {
        host_id: text(fingerprint, "hostId")?,
        provider: Provider::from_wire(fingerprint.get("provider")?.as_str()?)?,
        home: text(fingerprint, "resolvedHomePath")?,
        volume_id: fingerprint.get("volumeId")?.as_str()?.to_owned(),
        status,
        malformed: int(value, "malformedRecords")?,
        distinct_sessions: int(value, "distinctSessions")?,
        message,
        has_action,
        refreshing,
    })
}

fn valid_pricing(value: &Value) -> bool {
    let fetched = matches!(value.get("fetchedAt"), Some(Value::Null | Value::String(_)));
    matches!(
        value.get("status").and_then(Value::as_str),
        Some("fresh" | "cached" | "unavailable")
    ) && text(value, "source").is_some()
        && fetched
        && int(value, "knownModels").is_some()
}

impl Summary {
    /// Mirrors the `UsageSummary` schema; array elements that fail it are dropped, as
    /// `ForwardCompatibleArray` does.
    pub fn decode(value: &Value) -> Option<Self> {
        let contract_version = value.get("contractVersion")?.as_f64()?;
        value.get("readAt")?.as_str()?;
        let time_zone = text(value, "timeZone")?;
        text(value, "sinceDay").filter(|d| is_day(d))?;
        text(value, "untilDay").filter(|d| is_day(d))?;
        if !valid_pricing(value.get("pricing")?) {
            return None;
        }
        int(value, "scanDurationMs")?;
        let buckets = value.get("buckets")?.as_array()?;
        let sources = value.get("sources")?.as_array()?;
        Some(Self {
            contract_version: contract_version as u64,
            time_zone,
            buckets: buckets.iter().filter_map(decode_bucket).collect(),
            sources: sources.iter().filter_map(decode_source).collect(),
        })
    }

    pub fn refreshing(&self) -> bool {
        self.sources.iter().any(|s| s.refreshing)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Share {
    pub cost: f64,
    pub tokens: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderTotals {
    pub provider: Provider,
    pub cost: f64,
    pub tokens: f64,
    pub sessions: f64,
    pub cost_share: f64,
    pub token_share: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelTotals {
    pub model: String,
    pub provider: Provider,
    pub cost: f64,
    pub tokens: f64,
    pub records: f64,
    pub unpriced_records: f64,
    pub cost_share: f64,
    pub token_share: f64,
}

impl ModelTotals {
    /// When every record lacks rates, the cost is unknown and is never shown as a dollar figure.
    pub fn cost_unknown(&self) -> bool {
        self.records > 0.0 && self.unpriced_records >= self.records
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PeriodTotals {
    pub key: String,
    pub cost: f64,
    pub tokens: f64,
    pub by_provider: Vec<(Provider, Share)>,
}

impl PeriodTotals {
    fn provider(&self, provider: Provider) -> Option<Share> {
        self.by_provider
            .iter()
            .find(|(p, _)| *p == provider)
            .map(|(_, s)| *s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CategoryCost {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
    pub unsplit: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeedCost {
    pub standard: f64,
    pub fast: f64,
    pub ultrafast: f64,
    pub premium: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    ServerBehind,
    ClientBehind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Merged {
    pub cost: f64,
    pub tokens: Tokens,
    pub total_tokens: f64,
    pub sessions: f64,
    pub providers: Vec<ProviderTotals>,
    pub models: Vec<ModelTotals>,
    pub daily: Vec<PeriodTotals>,
    pub hourly: Vec<PeriodTotals>,
    pub unpriced_share: f64,
    pub cache_savings: f64,
    pub category_cost: CategoryCost,
    pub speed_cost: SpeedCost,
    pub mismatch: Option<Mismatch>,
}

/// `isCompatibleUsageContractVersion`, usageMerge.ts.
fn compatible(version: u64) -> bool {
    (MERGE_COMPATIBLE_SINCE..=CONTRACT_VERSION).contains(&version)
}

fn entry<'a, T>(list: &'a mut Vec<(String, T)>, key: &str, make: impl FnOnce() -> T) -> &'a mut T {
    let at = match list.iter().position(|(k, _)| k == key) {
        Some(at) => at,
        None => {
            list.push((key.to_owned(), make()));
            list.len() - 1
        }
    };
    &mut list[at].1
}

fn add_share(by_provider: &mut Vec<(Provider, Share)>, provider: Provider, cost: f64, tokens: f64) {
    let at = match by_provider.iter().position(|(p, _)| *p == provider) {
        Some(at) => at,
        None => {
            by_provider.push((provider, Share::default()));
            by_provider.len() - 1
        }
    };
    by_provider[at].1.cost += cost;
    by_provider[at].1.tokens += tokens;
}

fn ratio(part: f64, whole: f64) -> f64 {
    if whole == 0.0 { 0.0 } else { part / whole }
}

fn by_desc(a: f64, b: f64) -> std::cmp::Ordering {
    b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal)
}

/// `mergeUsage` (usageMerge.ts) for the one environment this app reads: the source claiming
/// order is ok, partial, failed, a repeated fingerprint is skipped, and only claimed
/// providers and source paths contribute buckets.
pub fn merge(summary: &Summary) -> Merged {
    if !compatible(summary.contract_version) {
        return Merged {
            mismatch: Some(if summary.contract_version < CONTRACT_VERSION {
                Mismatch::ServerBehind
            } else {
                Mismatch::ClientBehind
            }),
            ..Merged::default()
        };
    }

    let mut owned: Vec<([&str; 4], f64)> = Vec::new();
    for status in [
        SourceStatus::Ok,
        SourceStatus::Partial,
        SourceStatus::Failed,
    ] {
        for source in summary.sources.iter().filter(|s| s.status == status) {
            let key = source.key();
            if !owned.iter().any(|(k, _)| *k == key) {
                owned.push((key, source.distinct_sessions));
            }
        }
    }

    let mut owned_providers: Vec<Provider> = Vec::new();
    let mut owned_paths: Vec<(Provider, &str)> = Vec::new();
    let mut sessions_by_provider: Vec<(Provider, f64)> = Vec::new();
    for source in &summary.sources {
        if source.status == SourceStatus::Missing {
            continue;
        }
        let key = source.key();
        let Some((_, sessions)) = owned.iter().find(|(k, _)| *k == key) else {
            continue;
        };
        owned_providers.push(source.provider);
        owned_paths.push((source.provider, &source.home));
        match sessions_by_provider
            .iter_mut()
            .find(|(p, _)| *p == source.provider)
        {
            Some((_, total)) => *total += sessions,
            None => sessions_by_provider.push((source.provider, *sessions)),
        }
    }

    let mut merged = Merged::default();
    let mut records = 0.0;
    let mut unpriced_records = 0.0;
    let mut category = [0.0; 4];
    let mut providers: Vec<(Provider, ProviderTotals)> = Vec::new();
    let mut models: Vec<(String, ModelTotals)> = Vec::new();
    let mut daily: Vec<(String, PeriodTotals)> = Vec::new();
    let mut hourly: Vec<(String, PeriodTotals)> = Vec::new();

    let blank = |provider| ProviderTotals {
        provider,
        cost: 0.0,
        tokens: 0.0,
        sessions: 0.0,
        cost_share: 0.0,
        token_share: 0.0,
    };
    for (provider, sessions) in &sessions_by_provider {
        merged.sessions += sessions;
        if *sessions == 0.0 {
            continue;
        }
        providers.push((*provider, blank(*provider)));
        providers.last_mut().expect("pushed").1.sessions += sessions;
    }

    for bucket in &summary.buckets {
        let contributes = match &bucket.source_path {
            None => owned_providers.contains(&bucket.provider),
            Some(path) => owned_paths.contains(&(bucket.provider, path.as_str())),
        };
        if !contributes {
            continue;
        }
        let tokens = bucket.totals.total();
        merged.cost += bucket.cost_usd;
        merged.cache_savings += bucket.cache_savings_usd;
        merged.tokens.uncached += bucket.totals.uncached;
        merged.tokens.cached += bucket.totals.cached;
        merged.tokens.cache_creation += bucket.totals.cache_creation;
        merged.tokens.output += bucket.totals.output;
        merged.tokens.reasoning += bucket.totals.reasoning;
        records += bucket.records;
        unpriced_records += bucket.unpriced_records;
        if let Some(c) = bucket.category_cost {
            for (total, part) in category.iter_mut().zip(c) {
                *total += part;
            }
        }
        merged.speed_cost.fast += bucket.fast_cost;
        merged.speed_cost.ultrafast += bucket.ultrafast_cost;
        merged.speed_cost.premium += bucket.speed_premium;

        let at = match providers.iter().position(|(p, _)| *p == bucket.provider) {
            Some(at) => at,
            None => {
                providers.push((bucket.provider, blank(bucket.provider)));
                providers.len() - 1
            }
        };
        providers[at].1.cost += bucket.cost_usd;
        providers[at].1.tokens += tokens;

        let model_key = format!("{} {}", bucket.provider.driver_kind(), bucket.model);
        let model = entry(&mut models, &model_key, || ModelTotals {
            model: bucket.model.clone(),
            provider: bucket.provider,
            cost: 0.0,
            tokens: 0.0,
            records: 0.0,
            unpriced_records: 0.0,
            cost_share: 0.0,
            token_share: 0.0,
        });
        model.cost += bucket.cost_usd;
        model.tokens += tokens;
        model.records += bucket.records;
        model.unpriced_records += bucket.unpriced_records;

        let period = |key: &str| PeriodTotals {
            key: key.to_owned(),
            cost: 0.0,
            tokens: 0.0,
            by_provider: Vec::new(),
        };
        let day = entry(&mut daily, &bucket.day, || period(&bucket.day));
        day.cost += bucket.cost_usd;
        day.tokens += tokens;
        add_share(
            &mut day.by_provider,
            bucket.provider,
            bucket.cost_usd,
            tokens,
        );
        if let Some(hour_start) = &bucket.hour_start {
            let hour = entry(&mut hourly, hour_start, || period(hour_start));
            hour.cost += bucket.cost_usd;
            hour.tokens += tokens;
            add_share(
                &mut hour.by_provider,
                bucket.provider,
                bucket.cost_usd,
                tokens,
            );
        }
    }

    merged.total_tokens = merged.tokens.uncached
        + merged.tokens.cached
        + merged.tokens.cache_creation
        + merged.tokens.output;
    let (cost, total) = (merged.cost, merged.total_tokens);

    merged.providers = providers
        .into_iter()
        .map(|(_, mut p)| {
            p.cost_share = ratio(p.cost, cost);
            p.token_share = ratio(p.tokens, total);
            p
        })
        .collect();
    merged.providers.sort_by(|a, b| by_desc(a.cost, b.cost));
    merged.models = models
        .into_iter()
        .map(|(_, mut m)| {
            m.cost_share = ratio(m.cost, cost);
            m.token_share = ratio(m.tokens, total);
            m
        })
        .collect();
    merged
        .models
        .sort_by(|a, b| by_desc(a.cost, b.cost).then(by_desc(a.tokens, b.tokens)));
    merged.daily = daily.into_iter().map(|(_, d)| d).collect();
    merged.daily.sort_by(|a, b| a.key.cmp(&b.key));
    merged.hourly = hourly.into_iter().map(|(_, h)| h).collect();
    merged.hourly.sort_by(|a, b| a.key.cmp(&b.key));

    merged.unpriced_share = ratio(unpriced_records, records);
    merged.category_cost = CategoryCost {
        input: category[0],
        cache_read: category[1],
        cache_write: category[2],
        output: category[3],
        unsplit: (cost - category.iter().sum::<f64>()).max(0.0),
    };
    merged.speed_cost.standard =
        (cost - merged.speed_cost.fast - merged.speed_cost.ultrafast).max(0.0);
    merged
}

/// usageBreakdown.ts `sortModelsByTokens`.
pub fn sort_models_by_tokens(models: &[ModelTotals]) -> Vec<ModelTotals> {
    let mut sorted = models.to_vec();
    sorted.sort_by(|a, b| by_desc(a.tokens, b.tokens).then(by_desc(a.cost, b.cost)));
    sorted
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Cost,
    Tokens,
    Limits,
}

/// usageBreakdown.ts `modelShare`.
pub fn model_share(model: &ModelTotals, metric: Metric) -> Option<f64> {
    if metric == Metric::Tokens {
        return Some(model.token_share);
    }
    (!model.cost_unknown()).then_some(model.cost_share)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub label: &'static str,
    pub value: f64,
    /// Foreground over background, in percent (`ink` in usageBreakdown.ts).
    pub ink: u8,
}

pub fn cost_type_segments(cost: &CategoryCost) -> [Segment; 5] {
    let seg = |label, value, ink| Segment { label, value, ink };
    [
        seg("Input", cost.input, 60),
        seg("Cache read", cost.cache_read, 30),
        seg("Cache write", cost.cache_write, 72),
        seg("Output", cost.output, 100),
        seg(
            "Other",
            if cost.unsplit >= 0.005 {
                cost.unsplit
            } else {
                0.0
            },
            44,
        ),
    ]
}

pub fn token_type_segments(tokens: &Tokens) -> [Segment; 4] {
    let seg = |label, value, ink| Segment { label, value, ink };
    [
        seg("Input", tokens.uncached, 60),
        seg("Cache read", tokens.cached, 30),
        seg("Cache write", tokens.cache_creation, 72),
        seg("Output", tokens.output, 100),
    ]
}

pub fn speed_cost_segments(cost: &SpeedCost) -> [Segment; 3] {
    let seg = |label, value, ink| Segment { label, value, ink };
    [
        seg("Standard", cost.standard, 34),
        seg("Fast", cost.fast, 66),
        seg("Ultrafast", cost.ultrafast, 100),
    ]
}

/// `providersWithUsage`, usageProviders.ts.
pub fn active_providers(merged: &Merged) -> Vec<Provider> {
    Provider::ORDER
        .into_iter()
        .filter(|provider| {
            merged
                .providers
                .iter()
                .any(|p| p.provider == *provider && (p.tokens > 0.0 || p.cost > 0.0))
        })
        .collect()
}

// Formatting, packages/shared/src/usageFormat.ts.

fn group(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn round_half_up(value: f64) -> f64 {
    (value + 0.5).floor()
}

pub fn format_count(value: f64) -> String {
    let rounded = round_half_up(value);
    let sign = if rounded < 0.0 { "-" } else { "" };
    format!("{sign}{}", group(&format!("{:.0}", rounded.abs())))
}

pub fn format_usd(value: f64) -> String {
    let fixed = format!("{:.2}", value.abs());
    let (whole, cents) = fixed.split_once('.').unwrap_or((&fixed, "00"));
    let sign = if value < 0.0 && fixed != "0.00" {
        "-"
    } else {
        ""
    };
    format!("{sign}${}.{cents}", group(whole))
}

fn trim(value: f64) -> String {
    let abs = value.abs();
    let digits = if abs >= 100.0 {
        0
    } else if abs >= 10.0 {
        1
    } else {
        2
    };
    let fixed = format!("{value:.digits$}");
    match fixed.split_once('.') {
        Some((whole, frac)) if frac.bytes().all(|b| b == b'0') => whole.to_owned(),
        _ => fixed,
    }
}

pub fn format_tokens(value: f64) -> String {
    let abs = value.abs();
    for (floor, suffix) in [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")] {
        if abs >= floor {
            return format!("{}{suffix}", trim(value / floor));
        }
    }
    format_count(value)
}

pub fn format_percent(share: f64, digits: usize) -> String {
    let percent = share * 100.0;
    let smallest = 10f64.powi(-(digits as i32));
    if percent > 0.0 && percent < smallest {
        return format!("<{smallest:.digits$}%");
    }
    format!("{percent:.digits$}%")
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn format_day_short(day: &str) -> String {
    let mut parts = day.split('-').map(|p| p.parse::<usize>().ok());
    match (
        parts.next().flatten(),
        parts.next().flatten(),
        parts.next().flatten(),
    ) {
        (Some(_), Some(month), Some(day_of_month)) => format!(
            "{} {day_of_month}",
            month
                .checked_sub(1)
                .and_then(|i| MONTHS.get(i))
                .unwrap_or(&"")
        ),
        _ => day.to_owned(),
    }
}

pub fn malformed_warning(count: f64) -> String {
    format!(
        "{} malformed or inconsistent usage record(s) were skipped; usage may be incomplete.",
        count as u64
    )
}

/// `usageSourceMessages`, usageFormat.ts.
pub fn source_messages(sources: &[Source]) -> Vec<String> {
    let mut messages: Vec<String> = Vec::new();
    let mut malformed: Vec<([String; 4], f64)> = Vec::new();
    for source in sources {
        if source.has_action
            || (!matches!(source.status, SourceStatus::Partial | SourceStatus::Failed)
                && source.provider != Provider::Cursor
                && !(source.status == SourceStatus::Missing && source.malformed > 0.0))
        {
            continue;
        }
        let mut message = source.message.clone();
        if source.malformed > 0.0 {
            let key = source.key().map(str::to_owned);
            match malformed.iter_mut().find(|(k, _)| *k == key) {
                Some((_, count)) => *count = count.max(source.malformed),
                None => malformed.push((key, source.malformed)),
            }
            let warning = malformed_warning(source.malformed);
            if let Some(text) = &message
                && let Some(head) = text.strip_suffix(&warning)
            {
                message = Some(head.trim().to_owned());
            }
        }
        if let Some(text) = message
            && !text.is_empty()
            && !messages.contains(&text)
        {
            messages.push(text);
        }
    }
    let total: f64 = malformed.iter().map(|(_, c)| c).sum();
    if total > 0.0 {
        let warning = malformed_warning(total);
        if !messages.contains(&warning) {
            messages.push(warning);
        }
    }
    messages
}

pub fn contract_mismatch_message(label: &str, mismatch: Mismatch) -> String {
    match mismatch {
        Mismatch::ServerBehind => {
            format!("{label} runs an older server version and is excluded from totals.")
        }
        Mismatch::ClientBehind => {
            format!(
                "This client is older than the server on {label}; its usage is excluded from totals."
            )
        }
    }
}

// Calendar math. The web formats through Intl in the viewer's zone; the local zone here comes
// from libc.

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn format_day(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn iso_from_ms(ms: i64) -> String {
    let days = ms.div_euclid(DAY_MS);
    let rest = ms.rem_euclid(DAY_MS);
    format!(
        "{}T{:02}:{:02}:{:02}.{:03}Z",
        format_day(days),
        rest / HOUR_MS,
        rest % HOUR_MS / 60_000,
        rest % 60_000 / 1_000,
        rest % 1_000
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Zone {
    Utc,
    Local(String),
}

impl Zone {
    /// The IANA name from `TZ` or the `/etc/localtime` link, or UTC when neither names one.
    pub fn detect() -> Self {
        let from_env = std::env::var("TZ")
            .ok()
            .map(|tz| tz.trim_start_matches(':').to_owned())
            .filter(|tz| !tz.is_empty());
        let name = from_env.or_else(|| {
            let link = std::fs::read_link("/etc/localtime").ok()?;
            let link = link.to_string_lossy();
            link.split_once("zoneinfo/")
                .map(|(_, name)| name.to_owned())
        });
        match name {
            Some(name) if name != "UTC" => Self::Local(name),
            _ => Self::Utc,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Utc => "UTC",
            Self::Local(name) => name,
        }
    }

    fn offset_ms(&self, instant_ms: i64) -> i64 {
        match self {
            Self::Utc => 0,
            Self::Local(_) => {
                let secs: libc::time_t = instant_ms.div_euclid(1_000);
                // SAFETY: `tm` is plain data and `localtime_r` only writes through the pointers given.
                let tm = unsafe {
                    let mut tm: libc::tm = std::mem::zeroed();
                    libc::localtime_r(&secs, &mut tm);
                    tm
                };
                tm.tm_gmtoff * 1_000
            }
        }
    }

    fn civil(&self, instant_ms: i64) -> Civil {
        let local = instant_ms + self.offset_ms(instant_ms);
        let days = local.div_euclid(DAY_MS);
        let (_, month, day) = civil_from_days(days);
        Civil {
            days,
            month,
            day,
            hour: local.rem_euclid(DAY_MS) / HOUR_MS,
        }
    }
}

struct Civil {
    days: i64,
    month: i64,
    day: i64,
    hour: i64,
}

fn hour_label(hour: i64) -> String {
    let twelve = if hour % 12 == 0 { 12 } else { hour % 12 };
    format!("{twelve} {}", if hour < 12 { "AM" } else { "PM" })
}

/// `formatHourShort`, without the repeated-hour zone suffix.
pub fn format_hour_short(hour_start: &str, zone: &Zone) -> String {
    match timestamp_ms(hour_start) {
        Some(ms) => hour_label(zone.civil(ms).hour),
        None => hour_start.to_owned(),
    }
}

pub fn format_date_time_short(instant: &str, zone: &Zone) -> String {
    match timestamp_ms(instant) {
        Some(ms) => {
            let c = zone.civil(ms);
            format!(
                "{} {}, {}",
                MONTHS[(c.month - 1) as usize],
                c.day,
                hour_label(c.hour)
            )
        }
        None => instant.to_owned(),
    }
}

pub fn format_relative_hour_short(hour_start: &str, relative_to: &str, zone: &Zone) -> String {
    let (Some(instant), Some(reference)) = (timestamp_ms(hour_start), timestamp_ms(relative_to))
    else {
        return format_date_time_short(hour_start, zone);
    };
    let ago = zone.civil(reference).days - zone.civil(instant).days;
    let hour = format_hour_short(hour_start, zone);
    match ago {
        0 => format!("{hour} today"),
        1 => format!("{hour} yesterday"),
        _ => format_date_time_short(hour_start, zone),
    }
}

pub fn enumerate_days(since: &str, until: &str) -> Vec<String> {
    let parse = |day: &str| {
        let ms = timestamp_ms(&format!("{day}T00:00:00Z"))?;
        Some(ms.div_euclid(DAY_MS))
    };
    match (parse(since), parse(until)) {
        (Some(start), Some(end)) if end >= start => (start..=end).map(format_day).collect(),
        _ => Vec::new(),
    }
}

pub fn enumerate_hour_starts(since: &str, until: &str) -> Vec<String> {
    match (timestamp_ms(since), timestamp_ms(until)) {
        (Some(start), Some(end)) if end > start => (0..)
            .map(|i| start + i * HOUR_MS)
            .take_while(|t| *t < end)
            .map(iso_from_ms)
            .collect(),
        _ => Vec::new(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub since_day: String,
    pub until_day: String,
    pub time_zone: String,
    pub hourly: bool,
    pub since_time: Option<String>,
    pub until_time: Option<String>,
}

impl Window {
    /// The `UsageSummaryInput` payload.
    pub fn payload(&self, await_refresh: bool) -> Value {
        let mut payload = json!({
            "sinceDay": self.since_day,
            "untilDay": self.until_day,
            "timeZone": self.time_zone,
            "resolution": if self.hourly { "hour" } else { "day" },
        });
        if let (Some(since), Some(until)) = (&self.since_time, &self.until_time) {
            payload["sinceTime"] = json!(since);
            payload["untilTime"] = json!(until);
        }
        if await_refresh {
            payload["awaitRefresh"] = json!(true);
        }
        payload
    }
}

/// `makeWindow`, usageFormat.ts.
pub fn make_window(days: u32, now_ms: i64, hourly: bool, zone: &Zone) -> Window {
    let day_of = |ms| format_day(zone.civil(ms).days);
    if hourly {
        let until = now_ms.div_euclid(60_000) * 60_000;
        let since = until - 24 * HOUR_MS;
        return Window {
            since_day: day_of(since),
            until_day: day_of(until),
            time_zone: zone.name().to_owned(),
            hourly,
            since_time: Some(iso_from_ms(since)),
            until_time: Some(iso_from_ms(until)),
        };
    }
    let today = zone.civil(now_ms).days;
    Window {
        since_day: format_day(today - (i64::from(days) - 1)),
        until_day: format_day(today),
        time_zone: zone.name().to_owned(),
        hourly,
        since_time: None,
        until_time: None,
    }
}

// Chart math, UsageProviderChart.tsx.

pub const VIEW_WIDTH: f64 = 960.0;
pub const VIEW_HEIGHT: f64 = 260.0;
pub const TICK_COUNT: usize = 4;
pub const PLOT_TOP: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub from: Point,
    pub c1: Point,
    pub c2: Point,
    pub to: Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub bands: Vec<(Provider, f64)>,
    pub total: f64,
}

pub fn build_period_columns(
    periods: &[String],
    totals: &[PeriodTotals],
    metric: Metric,
) -> Vec<Column> {
    periods
        .iter()
        .map(|period| {
            let entry = totals.iter().find(|t| t.key == *period);
            let bands: Vec<(Provider, f64)> = Provider::ORDER
                .iter()
                .map(|provider| {
                    let value = entry
                        .and_then(|e| e.provider(*provider))
                        .map_or(0.0, |share| {
                            if metric == Metric::Tokens {
                                share.tokens
                            } else {
                                share.cost
                            }
                        });
                    (*provider, value)
                })
                .collect();
            let total = bands.iter().map(|(_, v)| v).sum();
            Column { bands, total }
        })
        .collect()
}

fn monotone_tangents(points: &[Point]) -> Vec<f64> {
    let count = points.len();
    if count < 2 {
        return vec![0.0];
    }
    let slopes: Vec<f64> = points
        .windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            if dx == 0.0 {
                0.0
            } else {
                (w[1].y - w[0].y) / dx
            }
        })
        .collect();
    let mut tangents = vec![0.0; count];
    tangents[0] = slopes[0];
    tangents[count - 1] = slopes[count - 2];
    for i in 1..count - 1 {
        let (previous, next) = (slopes[i - 1], slopes[i]);
        tangents[i] = if previous * next <= 0.0 {
            0.0
        } else {
            (previous + next) / 2.0
        };
    }
    for i in 0..count - 1 {
        let slope = slopes[i];
        if slope == 0.0 {
            tangents[i] = 0.0;
            tangents[i + 1] = 0.0;
            continue;
        }
        let a = tangents[i] / slope;
        let b = tangents[i + 1] / slope;
        let magnitude = a * a + b * b;
        if magnitude > 9.0 {
            let scale = 3.0 / magnitude.sqrt();
            tangents[i] = scale * a * slope;
            tangents[i + 1] = scale * b * slope;
        }
    }
    tangents
}

pub fn smooth_curve(points: &[Point]) -> Vec<Curve> {
    if points.len() < 2 {
        return Vec::new();
    }
    let tangents = monotone_tangents(points);
    points
        .windows(2)
        .enumerate()
        .map(|(i, w)| {
            let dx = w[1].x - w[0].x;
            Curve {
                from: w[0],
                c1: Point {
                    x: w[0].x + dx / 3.0,
                    y: w[0].y + tangents[i] * dx / 3.0,
                },
                c2: Point {
                    x: w[1].x - dx / 3.0,
                    y: w[1].y - tangents[i + 1] * dx / 3.0,
                },
                to: w[1],
            }
        })
        .collect()
}

pub fn nice_scale(peak: f64, count: usize) -> (f64, Vec<f64>) {
    if peak <= 0.0 {
        return (0.0, vec![0.0]);
    }
    let raw_step = peak / count as f64;
    let magnitude = 10f64.powf(raw_step.log10().floor());
    let normalized = raw_step / magnitude;
    let factor = if normalized > 5.0 {
        10.0
    } else if normalized > 2.0 {
        5.0
    } else if normalized > 1.0 {
        2.0
    } else {
        1.0
    };
    let step = factor * magnitude;
    let max = (peak / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = 0.0;
    while value <= max + step * 1e-6 {
        ticks.push(value);
        value += step;
    }
    (max, ticks)
}

pub fn value_to_y(value: f64, max: f64) -> f64 {
    if max == 0.0 {
        VIEW_HEIGHT
    } else {
        VIEW_HEIGHT - (value / max) * (VIEW_HEIGHT - PLOT_TOP)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub provider: Provider,
    pub total: f64,
    pub curve: Vec<Curve>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub columns: Vec<Column>,
    pub max: f64,
    pub ticks: Vec<f64>,
    pub step_x: f64,
    pub series: Vec<Series>,
}

pub fn build_chart(
    periods: &[String],
    totals: &[PeriodTotals],
    metric: Metric,
    providers: &[Provider],
) -> Chart {
    let columns = build_period_columns(periods, totals, metric);
    let peak = columns
        .iter()
        .flat_map(|c| c.bands.iter().map(|(_, v)| *v))
        .fold(0.0, f64::max);
    let (max, ticks) = nice_scale(peak, TICK_COUNT);
    let step_x = if periods.len() < 2 {
        0.0
    } else {
        VIEW_WIDTH / (periods.len() - 1) as f64
    };
    let mut series: Vec<Series> = providers
        .iter()
        .map(|provider| {
            let slot = provider.slot();
            let points: Vec<Point> = columns
                .iter()
                .enumerate()
                .map(|(i, c)| Point {
                    x: i as f64 * step_x,
                    y: value_to_y(c.bands[slot].1, max),
                })
                .collect();
            Series {
                provider: *provider,
                total: columns.iter().map(|c| c.bands[slot].1).sum(),
                curve: smooth_curve(&points),
            }
        })
        .collect();
    series.sort_by(|a, b| by_desc(a.total, b.total));
    Chart {
        columns,
        max,
        ticks,
        step_x,
        series,
    }
}

// Preferences, usagePagePreferences.ts. The web keeps them in localStorage; this app has no
// such store, so they live in a small JSON file under the app's home directory.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub metric: Metric,
    pub window_days: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            metric: Metric::Limits,
            window_days: 30,
        }
    }
}

const PREFS_FILE: &str = "usage-page-preferences.v1.json";

impl Prefs {
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let metric = match value.get("metric")?.as_str()? {
            "cost" => Metric::Cost,
            "tokens" => Metric::Tokens,
            "limits" => Metric::Limits,
            _ => return None,
        };
        let window_days = match value.get("windowDays")?.as_u64()? {
            n @ (1 | 7 | 30 | 90) => n as u32,
            _ => return None,
        };
        Some(Self {
            metric,
            window_days,
        })
    }

    pub fn to_json(self) -> String {
        json!({
            "metric": match self.metric {
                Metric::Cost => "cost",
                Metric::Tokens => "tokens",
                Metric::Limits => "limits",
            },
            "windowDays": self.window_days,
        })
        .to_string()
    }

    pub fn path() -> Option<PathBuf> {
        let home = std::env::var(crate::config::ENV_HOME).ok()?;
        (!home.trim().is_empty()).then(|| Path::new(&home).join(PREFS_FILE))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(self) {
        if let Some(path) = Self::path() {
            let _ = std::fs::write(path, self.to_json());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(overrides: impl FnOnce(&mut Value)) -> Value {
        let mut value = json!({
            "day": "2026-08-07",
            "provider": "claude",
            "model": "claude-fable-5",
            "totals": {
                "uncachedInputTokens": 100,
                "cachedInputTokens": 1000,
                "cacheCreationTokens": 10,
                "outputTokens": 50,
                "reasoningTokens": 0,
            },
            "costUsd": 10,
            "cacheSavingsUsd": 2,
            "costSource": "modelPriced",
            "records": 5,
            "unpricedRecords": 0,
            "sessions": 1,
        });
        overrides(&mut value);
        value
    }

    fn summary(buckets: Vec<Value>, sources: Vec<(&str, &str, u64)>, version: u64) -> Value {
        json!({
            "contractVersion": version,
            "readAt": "2026-08-07T00:00:00.000Z",
            "timeZone": "UTC",
            "sinceDay": "2026-08-01",
            "untilDay": "2026-08-31",
            "buckets": buckets,
            "sources": sources.into_iter().map(|(provider, home, sessions)| json!({
                "fingerprint": {
                    "hostId": "mac",
                    "provider": provider,
                    "resolvedHomePath": home,
                    "volumeId": "vol-mac",
                },
                "status": "ok",
                "scannedFiles": 1,
                "skippedFiles": 0,
                "malformedRecords": 0,
                "distinctSessions": sessions,
                "message": null,
            })).collect::<Vec<_>>(),
            "pricing": {"status": "fresh", "source": "litellm", "fetchedAt": null, "knownModels": 10},
            "scanDurationMs": 1,
        })
    }

    fn merged(value: Value) -> Merged {
        merge(&Summary::decode(&value).expect("decodes"))
    }

    fn source(
        provider: Provider,
        status: SourceStatus,
        malformed: f64,
        message: Option<&str>,
    ) -> Source {
        Source {
            host_id: "demo-host".into(),
            provider,
            home: "/demo/codex".into(),
            volume_id: "demo-volume".into(),
            status,
            malformed,
            distinct_sessions: 1.0,
            message: message.map(str::to_owned),
            has_action: false,
            refreshing: false,
        }
    }

    fn malformed(count: f64) -> Source {
        source(
            Provider::Codex,
            SourceStatus::Partial,
            count,
            Some(&malformed_warning(count)),
        )
    }

    #[test]
    fn formats_money_counts_tokens_and_percent() {
        assert_eq!(format_usd(12_076.08), "$12,076.08");
        assert_eq!(format_usd(0.0), "$0.00");
        assert_eq!(format_count(3_687.0), "3,687");
        assert_eq!(format_tokens(101e9), "101B");
        assert_eq!(format_tokens(769e6), "769M");
        assert_eq!(format_tokens(1_500.0), "1.50K");
        assert_eq!(format_tokens(12_340.0), "12.3K");
        assert_eq!(format_tokens(999.0), "999");
        assert_eq!(format_tokens(2e12), "2T");
        assert_eq!(format_percent(0.529, 1), "52.9%");
        assert_eq!(format_percent(0.0004, 1), "<0.1%");
        assert_eq!(format_percent(0.0, 1), "0.0%");
        assert_eq!(format_day_short("2026-08-11"), "Aug 11");
    }

    #[test]
    fn combines_malformed_warnings() {
        let warning = |n| malformed_warning(n);
        let other_host = |count, host: &str| Source {
            host_id: host.into(),
            ..malformed(count)
        };
        assert_eq!(
            source_messages(&[
                malformed(1.0),
                Source {
                    provider: Provider::Claude,
                    home: "/demo/claude".into(),
                    ..malformed(11.0)
                }
            ]),
            vec![warning(12.0)]
        );
        let (first, second) = (malformed(1.0), other_host(11.0, "other-demo-host"));
        assert_eq!(
            source_messages(&[first.clone(), first.clone(), second.clone(), second.clone()]),
            vec![warning(12.0)]
        );
        assert_eq!(source_messages(&[first.clone(), first]), vec![warning(1.0)]);
        let machine = Source {
            volume_id: "other-demo-volume".into(),
            ..malformed(11.0)
        };
        assert_eq!(
            source_messages(&[malformed(1.0), machine]),
            vec![warning(12.0)]
        );
        assert_eq!(
            source_messages(&[malformed(1.0), malformed(11.0), malformed(1.0)]),
            vec![warning(11.0)]
        );
    }

    #[test]
    fn keeps_read_failures_beside_the_combined_warning() {
        let read_failure = "2 transcript path(s) could not be read; usage may be incomplete.";
        let first = malformed(1.0);
        let with_failure = Source {
            message: Some(format!("{read_failure} {}", malformed_warning(1.0))),
            ..first.clone()
        };
        let plain = Source {
            malformed: 0.0,
            message: Some(read_failure.into()),
            ..first.clone()
        };
        let cursor = Source {
            provider: Provider::Cursor,
            status: SourceStatus::Ok,
            malformed: 0.0,
            message: Some("Cursor history is limited to the last 30 days.".into()),
            ..first
        };
        assert_eq!(
            source_messages(&[with_failure, plain, cursor]),
            vec![
                read_failure.to_owned(),
                "Cursor history is limited to the last 30 days.".to_owned(),
                malformed_warning(1.0),
            ]
        );
    }

    #[test]
    fn missing_directories_keep_their_cached_diagnostics() {
        let missing = Source {
            host_id: "other-demo-host".into(),
            status: SourceStatus::Missing,
            malformed: 2.0,
            distinct_sessions: 0.0,
            message: Some(format!(
                "No transcript directory on this environment. {}",
                malformed_warning(2.0)
            )),
            ..malformed(2.0)
        };
        assert_eq!(
            source_messages(&[malformed(1.0), missing.clone(), missing]),
            vec![
                "No transcript directory on this environment.".to_owned(),
                malformed_warning(3.0),
            ]
        );
    }

    #[test]
    fn omits_empty_missing_healthy_and_action_sources() {
        let ok = |malformed, status| Source {
            status,
            malformed,
            ..self::malformed(0.0)
        };
        assert!(
            source_messages(&[
                ok(0.0, SourceStatus::Missing),
                ok(1.0, SourceStatus::Ok),
                Source {
                    has_action: true,
                    ..ok(1.0, SourceStatus::Partial)
                },
            ])
            .is_empty()
        );
        assert!(source_messages(&[]).is_empty());
    }

    #[test]
    fn builds_an_exact_minute_aligned_24_hour_request() {
        let now = timestamp_ms("2026-08-11T12:37:42.123Z").expect("time");
        let window = make_window(1, now, true, &Zone::Utc);
        assert_eq!(
            window.since_time.as_deref(),
            Some("2026-08-10T12:37:00.000Z")
        );
        assert_eq!(
            window.until_time.as_deref(),
            Some("2026-08-11T12:37:00.000Z")
        );
        assert_eq!(window.payload(false)["resolution"], "hour");
        assert_eq!(window.payload(true)["awaitRefresh"], true);
    }

    #[test]
    fn day_windows_end_today_and_span_the_requested_days() {
        let now = timestamp_ms("2026-03-02T12:37:42.123Z").expect("time");
        let window = make_window(7, now, false, &Zone::Utc);
        assert_eq!(
            (window.since_day.as_str(), window.until_day.as_str()),
            ("2026-02-24", "2026-03-02")
        );
        assert_eq!(window.time_zone, "UTC");
        assert_eq!(
            enumerate_days(&window.since_day, &window.until_day).len(),
            7
        );
        assert_eq!(
            make_window(30, now, false, &Zone::Utc).since_day,
            "2026-02-01"
        );
    }

    #[test]
    fn enumerates_hours_and_labels_them() {
        let hours = enumerate_hour_starts("2026-08-10T12:37:00.000Z", "2026-08-11T12:37:00.000Z");
        assert_eq!(hours.len(), 24);
        assert_eq!(hours[0], "2026-08-10T12:37:00.000Z");
        assert_eq!(
            format_hour_short("2026-08-11T12:37:00.000Z", &Zone::Utc),
            "12 PM"
        );
        assert_eq!(
            format_hour_short("2026-08-11T00:37:00.000Z", &Zone::Utc),
            "12 AM"
        );
        assert_eq!(format_hour_short("invalid", &Zone::Utc), "invalid");
        assert_eq!(
            format_date_time_short("2026-08-11T13:00:00.000Z", &Zone::Utc),
            "Aug 11, 1 PM"
        );
        let now = "2026-08-11T20:00:00.000Z";
        assert_eq!(
            format_relative_hour_short("2026-08-11T13:00:00.000Z", now, &Zone::Utc),
            "1 PM today"
        );
        assert_eq!(
            format_relative_hour_short("2026-08-10T13:00:00.000Z", now, &Zone::Utc),
            "1 PM yesterday"
        );
        assert_eq!(
            format_relative_hour_short("2026-08-08T13:00:00.000Z", now, &Zone::Utc),
            "Aug 8, 1 PM"
        );
    }

    #[test]
    fn keeps_known_usage_when_newer_variants_cannot_be_decoded() {
        let mut value = summary(vec![bucket(|_| {})], vec![("claude", "/a", 1)], 6);
        value["buckets"].as_array_mut().expect("array").extend([
            bucket(|b| {
                b["provider"] = json!("future-provider");
                b["costUsd"] = json!(100);
            }),
            bucket(|b| {
                b["costSource"] = json!("future-pricing");
                b["costUsd"] = json!(200);
            }),
        ]);
        let mut extra = value["sources"][0].clone();
        extra["fingerprint"]["provider"] = json!("future-provider");
        value["sources"].as_array_mut().expect("array").push(extra);
        let decoded = Summary::decode(&value).expect("decodes");
        assert_eq!((decoded.buckets.len(), decoded.sources.len()), (1, 1));
        let merged = merge(&decoded);
        assert_eq!(merged.cost, 10.0);
        assert_eq!(merged.total_tokens, 1160.0);
        assert_eq!(merged.mismatch, None);
    }

    #[test]
    fn trims_model_names_before_grouping() {
        let value = summary(
            vec![
                bucket(|b| {
                    b["provider"] = json!("codex");
                    b["model"] = json!(" gpt-5 ");
                }),
                bucket(|b| {
                    b["provider"] = json!("codex");
                    b["model"] = json!("gpt-5");
                }),
            ],
            vec![("codex", "/a", 1)],
            6,
        );
        let merged = merged(value);
        assert_eq!(merged.models.len(), 1);
        assert_eq!(merged.models[0].model, "gpt-5");
        assert_eq!(
            (merged.models[0].cost, merged.models[0].tokens),
            (20.0, 2320.0)
        );
    }

    #[test]
    fn flags_contract_versions_outside_the_compatible_range() {
        let behind = merged(summary(vec![bucket(|_| {})], vec![("claude", "/a", 1)], 3));
        assert_eq!(behind.mismatch, Some(Mismatch::ServerBehind));
        assert_eq!(behind.cost, 0.0);
        let ahead = merged(summary(vec![bucket(|_| {})], vec![("claude", "/a", 1)], 7));
        assert_eq!(ahead.mismatch, Some(Mismatch::ClientBehind));
        let previous = merged(summary(vec![bucket(|_| {})], vec![("claude", "/a", 1)], 4));
        assert_eq!(previous.mismatch, None);
        assert_eq!(previous.cost, 10.0);
    }

    #[test]
    fn marks_a_model_with_no_known_rates_as_unpriced() {
        let value = summary(
            vec![
                bucket(|b| b["costUsd"] = json!(75)),
                bucket(|b| {
                    b["provider"] = json!("codex");
                    b["model"] = json!("unknown-model");
                    b["costUsd"] = json!(0);
                    b["costSource"] = json!("unpriced");
                    b["unpricedRecords"] = json!(5);
                }),
            ],
            vec![("claude", "/a/.claude", 1), ("codex", "/a/.codex", 1)],
            6,
        );
        let merged = merged(value);
        let unknown: Vec<&str> = merged
            .models
            .iter()
            .filter(|m| m.cost_unknown())
            .map(|m| m.model.as_str())
            .collect();
        assert_eq!(unknown, ["unknown-model"]);
        let model = &merged.models[1];
        assert_eq!(model_share(model, Metric::Cost), None);
        assert_eq!(model_share(model, Metric::Tokens), Some(model.token_share));
    }

    #[test]
    fn splits_cost_by_category_and_speed() {
        let value = summary(
            vec![
                bucket(|b| {
                    b["costUsd"] = json!(10);
                    b["categoryCostUsd"] =
                        json!({"input": 1, "cacheRead": 2, "cacheWrite": 3, "output": 4});
                    b["fastCostUsd"] = json!(6);
                    b["speedPremiumUsd"] = json!(3);
                }),
                bucket(|b| {
                    b["provider"] = json!("codex");
                    b["model"] = json!("unknown-model");
                    b["costUsd"] = json!(0);
                    b["costSource"] = json!("unpriced");
                    b["unpricedRecords"] = json!(5);
                }),
                bucket(|b| {
                    b["provider"] = json!("codex");
                    b["model"] = json!("partly-reported");
                    b["costUsd"] = json!(0);
                    b["unpricedRecords"] = json!(4);
                }),
            ],
            vec![("claude", "/a/.claude", 1), ("codex", "/a/.codex", 1)],
            6,
        );
        let merged = merged(value);
        assert_eq!(
            merged.category_cost,
            CategoryCost {
                input: 1.0,
                cache_read: 2.0,
                cache_write: 3.0,
                output: 4.0,
                unsplit: 0.0
            }
        );
        assert_eq!(
            merged.speed_cost,
            SpeedCost {
                standard: 4.0,
                fast: 6.0,
                ultrafast: 0.0,
                premium: 3.0
            }
        );
        assert!((merged.unpriced_share - 9.0 / 15.0).abs() < 1e-12);
    }

    #[test]
    fn unsplit_cost_is_the_remainder_when_a_server_sends_no_categories() {
        let value = summary(
            vec![
                bucket(|b| {
                    b["categoryCostUsd"] =
                        json!({"input": 1, "cacheRead": 2, "cacheWrite": 3, "output": 4});
                }),
                bucket(|b| b["costUsd"] = json!(5)),
            ],
            vec![("claude", "/a", 1)],
            6,
        );
        let merged = merged(value);
        assert_eq!(merged.category_cost.unsplit, 5.0);
        assert_eq!(merged.speed_cost.standard, 15.0);
    }

    #[test]
    fn totals_sessions_from_directory_counts_not_bucket_sums() {
        let value = summary(
            vec![
                bucket(|b| b["day"] = json!("2026-08-06")),
                bucket(|b| b["day"] = json!("2026-08-07")),
            ],
            vec![("claude", "/a/.claude", 1)],
            6,
        );
        let merged = merged(value);
        assert_eq!(merged.sessions, 1.0);
        assert_eq!(merged.providers[0].sessions, 1.0);
        assert_eq!(merged.daily.len(), 2);
    }

    #[test]
    fn buckets_from_unclaimed_directories_do_not_count() {
        let value = summary(
            vec![
                bucket(|b| b["sourcePath"] = json!("/a/.claude")),
                bucket(|b| b["sourcePath"] = json!("/elsewhere")),
            ],
            vec![("claude", "/a/.claude", 1)],
            6,
        );
        assert_eq!(merged(value).cost, 10.0);
    }

    #[test]
    fn sorts_models_by_cost_then_tokens_and_by_tokens_then_cost() {
        let value = summary(
            vec![
                bucket(|b| {
                    b["model"] = json!("cheap-big");
                    b["costUsd"] = json!(1);
                    b["totals"]["outputTokens"] = json!(1_000_000);
                }),
                bucket(|b| {
                    b["model"] = json!("pricey-small");
                    b["costUsd"] = json!(5);
                }),
            ],
            vec![("claude", "/a", 1)],
            6,
        );
        let merged = merged(value);
        let names = |models: &[ModelTotals]| -> Vec<String> {
            models.iter().map(|m| m.model.clone()).collect()
        };
        assert_eq!(names(&merged.models), ["pricey-small", "cheap-big"]);
        assert_eq!(
            names(&sort_models_by_tokens(&merged.models)),
            ["cheap-big", "pricey-small"]
        );
    }

    #[test]
    fn segments_drop_sub_cent_remainders() {
        let cost = CategoryCost {
            input: 1.0,
            unsplit: 0.004,
            ..CategoryCost::default()
        };
        let segments = cost_type_segments(&cost);
        assert_eq!(segments[4].value, 0.0);
        assert_eq!(segments[0].ink, 60);
        let tokens = token_type_segments(&Tokens {
            output: 3.0,
            ..Tokens::default()
        });
        assert_eq!(
            (tokens[3].label, tokens[3].value, tokens[3].ink),
            ("Output", 3.0, 100)
        );
        assert_eq!(speed_cost_segments(&SpeedCost::default())[1].ink, 66);
    }

    fn day_totals(day: &str, shares: &[(Provider, f64, f64)]) -> PeriodTotals {
        PeriodTotals {
            key: day.into(),
            cost: shares.iter().map(|s| s.1).sum(),
            tokens: shares.iter().map(|s| s.2).sum(),
            by_provider: shares
                .iter()
                .map(|(p, cost, tokens)| {
                    (
                        *p,
                        Share {
                            cost: *cost,
                            tokens: *tokens,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn period_columns_keep_band_values_absolute() {
        let days: Vec<String> = ["2026-08-01", "2026-08-02", "2026-08-03"]
            .map(String::from)
            .into();
        let totals = [
            day_totals(
                "2026-08-01",
                &[
                    (Provider::Codex, 10.0, 100.0),
                    (Provider::Claude, 20.0, 200.0),
                    (Provider::Pi, 5.0, 50.0),
                    (Provider::Muse, 7.0, 70.0),
                ],
            ),
            day_totals("2026-08-03", &[(Provider::Claude, 5.0, 50.0)]),
        ];
        let columns = build_period_columns(&days, &totals, Metric::Cost);
        let values: Vec<f64> = columns[0].bands.iter().map(|b| b.1).collect();
        assert_eq!(values, [10.0, 20.0, 0.0, 0.0, 0.0, 0.0, 5.0, 7.0]);
        assert_eq!(columns[0].bands[1].0, Provider::Claude);
        assert_eq!(columns[0].total, 42.0);
        assert_eq!(columns[1].total, 0.0);
        assert_eq!(
            build_period_columns(&days, &totals, Metric::Tokens)[2].total,
            50.0
        );
    }

    #[test]
    fn nice_scale_picks_round_steps() {
        assert_eq!(nice_scale(0.0, 4), (0.0, vec![0.0]));
        assert_eq!(nice_scale(9.0, 4), (10.0, vec![0.0, 5.0, 10.0]));
        let (max, ticks) = nice_scale(7.9e9, 4);
        assert_eq!(max, 8e9);
        assert_eq!(ticks, [0.0, 2e9, 4e9, 6e9, 8e9]);
        assert_eq!(nice_scale(100.0, 4).0, 100.0);
        assert_eq!(nice_scale(1.0, 4).1.len(), 3);
    }

    #[test]
    fn curves_stay_monotone_and_flat_runs_stay_flat() {
        let point = |x, y| Point { x, y };
        let flat = smooth_curve(&[point(0.0, 5.0), point(1.0, 5.0), point(2.0, 5.0)]);
        assert!(flat.iter().all(|c| c.c1.y == 5.0 && c.c2.y == 5.0));
        let rising = smooth_curve(&[
            point(0.0, 10.0),
            point(1.0, 5.0),
            point(2.0, 0.0),
            point(3.0, 0.0),
        ]);
        for c in &rising {
            let (low, high) = (c.from.y.min(c.to.y), c.from.y.max(c.to.y));
            assert!(c.c1.y >= low - 1e-9 && c.c1.y <= high + 1e-9);
            assert!(c.c2.y >= low - 1e-9 && c.c2.y <= high + 1e-9);
        }
        assert!(smooth_curve(&[point(0.0, 1.0)]).is_empty());
    }

    #[test]
    fn chart_scales_to_the_largest_single_series_value() {
        let days: Vec<String> = ["2026-08-01", "2026-08-02"].map(String::from).into();
        let totals = [
            day_totals(
                "2026-08-01",
                &[(Provider::Codex, 0.0, 1e9), (Provider::Claude, 0.0, 3e9)],
            ),
            day_totals("2026-08-02", &[(Provider::Claude, 0.0, 2e9)]),
        ];
        let chart = build_chart(
            &days,
            &totals,
            Metric::Tokens,
            &[Provider::Codex, Provider::Claude],
        );
        assert_eq!(chart.max, 3e9);
        assert_eq!(chart.step_x, VIEW_WIDTH);
        assert_eq!(chart.series[0].provider, Provider::Claude);
        assert_eq!(chart.series[0].curve[0].from.y, value_to_y(3e9, 3e9));
        assert_eq!(value_to_y(0.0, 0.0), VIEW_HEIGHT);
        assert_eq!(value_to_y(3e9, 3e9), PLOT_TOP);
    }

    #[test]
    fn active_providers_follow_the_fixed_order() {
        let value = summary(
            vec![bucket(|_| {}), bucket(|b| b["provider"] = json!("codex"))],
            vec![("claude", "/a", 1), ("codex", "/b", 1)],
            6,
        );
        assert_eq!(
            active_providers(&merged(value)),
            [Provider::Codex, Provider::Claude]
        );
    }

    #[test]
    fn preferences_default_to_limits_over_30_days_and_reject_bad_files() {
        assert_eq!(
            Prefs::default(),
            Prefs {
                metric: Metric::Limits,
                window_days: 30
            }
        );
        let saved = Prefs {
            metric: Metric::Cost,
            window_days: 7,
        };
        assert_eq!(Prefs::parse(&saved.to_json()), Some(saved));
        assert_eq!(Prefs::parse(r#"{"metric":"cost","windowDays":14}"#), None);
        assert_eq!(Prefs::parse(r#"{"metric":"nope","windowDays":7}"#), None);
        assert_eq!(Prefs::parse("not json"), None);
    }

    #[test]
    fn decoding_rejects_a_summary_missing_required_fields() {
        let mut value = summary(vec![], vec![], 6);
        assert!(Summary::decode(&value).is_some());
        value["pricing"]["status"] = json!("weird");
        assert!(Summary::decode(&value).is_none());
        assert!(Summary::decode(&json!({})).is_none());
    }

    #[test]
    fn contract_mismatch_messages_name_the_environment() {
        assert_eq!(
            contract_mismatch_message("Local", Mismatch::ServerBehind),
            "Local runs an older server version and is excluded from totals."
        );
        assert_eq!(
            contract_mismatch_message("Local", Mismatch::ClientBehind),
            "This client is older than the server on Local; its usage is excluded from totals."
        );
    }
}
