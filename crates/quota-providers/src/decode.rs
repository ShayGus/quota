//! The decoding rules every provider mapping shares.
//!
//! A provider mapping only turns its own wire payload into window drafts. The
//! decisions that must not drift between providers live here once: how a window
//! identity is derived, which measurement stands for a missing or unusable
//! number, what a reported boundary means, and how an unverified account is
//! labelled.

use chrono::{DateTime, Duration, Utc};
use quota_core::ports::{ConnectionBinding, FetchOutcome, ProviderError, QuotaRead};
use quota_domain::account::VerifiedIdentity;
use quota_domain::ids::{DefinitionVersion, MAX_ID_LEN, QuotaPoolId, QuotaWindowId, ResourceId};
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, PercentageMeasurement, UnavailableReason};
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{
    Boundary, BoundaryKind, Completeness, Enforcement, MetricRole, QuotaCategory, QuotaWindow,
    SourceKind, WindowSemantics,
};
use serde::Deserialize;

/// The detail text used when a scope cannot be represented at all.
const SCOPE_DETAIL: &str = "a quota scope could not be represented";
const IDENTITY_DETAIL: &str = "the payload carried no usable account identity";

/// A number as an undocumented provider wrote it: a JSON number or a string.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Numberish {
    /// A real JSON number.
    Number(serde_json::Number),
    /// A number written as text.
    Text(String),
}

/// A numeric field, with the decimal places the provider used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NumberField {
    /// The decoded value.
    pub(crate) value: f64,
    /// Decimal places in the text the provider sent, capped at the domain limit.
    pub(crate) decimals: DecimalPrecision,
}

impl Numberish {
    /// Decodes the field, rejecting text that is not a finite number.
    pub(crate) fn field(&self) -> Option<NumberField> {
        let text = match self {
            Self::Number(number) => number.to_string(),
            Self::Text(text) => text.trim().to_owned(),
        };
        let value: f64 = text.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        // A count above the domain ceiling is clamped, so this never fails.
        let decimals = DecimalPrecision::new(decimal_places(&text)).ok()?;
        Some(NumberField { value, decimals })
    }

    /// Decodes an integral count, rejecting fractional values.
    pub(crate) fn whole(&self) -> Option<i64> {
        let field = self.field()?;
        if field.value.fract().abs() > f64::EPSILON {
            return None;
        }
        if field.value.abs() >= 9_007_199_254_740_992.0 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the value is integral and inside the exact f64 integer range"
        )]
        Some(field.value.trunc() as i64)
    }

    /// The text form, when the provider sent one.
    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Self::Number(_) => None,
            Self::Text(text) => Some(text.as_str()),
        }
    }
}

/// The decimal places in the text form of a number, ignoring an exponent form.
fn decimal_places(text: &str) -> u8 {
    if text.contains(['e', 'E']) {
        return 0;
    }
    let Some((_, fraction)) = text.split_once('.') else {
        return 0;
    };
    u8::try_from(fraction.len())
        .unwrap_or(DecimalPrecision::MAX)
        .min(DecimalPrecision::MAX)
}

/// Reads a reset instant written as epoch seconds or as a date string.
///
/// A value outside the representable range, or a time that is not a date at
/// all, yields `None`: this crate never guesses a reset time.
pub(crate) fn reset_instant(value: &Numberish) -> Option<DateTime<Utc>> {
    match value {
        Numberish::Number(_) => epoch_instant(value.whole()?),
        Numberish::Text(text) => {
            let text = text.trim();
            if let Ok(seconds) = text.parse::<i64>() {
                return epoch_instant(seconds);
            }
            DateTime::parse_from_rfc3339(text)
                .or_else(|_| DateTime::parse_from_rfc2822(text))
                .ok()
                .map(|parsed| parsed.with_timezone(&Utc))
        }
    }
}

/// Converts epoch seconds into an instant, or `None` when out of range.
fn epoch_instant(seconds: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(seconds, 0)
}

/// Reads a reset instant written as a delay in seconds from receipt.
pub(crate) fn reset_after(value: &Numberish, received_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let seconds = value.whole()?;
    received_at.checked_add_signed(Duration::seconds(seconds.max(0)))
}

/// Reads a duration, converting from a per-unit scale when the source needs it.
pub(crate) fn scaled_seconds(value: &Numberish, per_unit: i64) -> Option<i64> {
    value.whole()?.checked_mul(per_unit)
}

/// The boundary kind a window's own semantics prove.
///
/// Only a bucket whose name states incremental replenishment earns that label.
/// Every other reported time is an unspecified provider event, because these
/// sources do not say what their boundaries mean.
pub(crate) fn boundary_kind(semantics: WindowSemantics) -> BoundaryKind {
    match semantics {
        WindowSemantics::RollingPeriod => BoundaryKind::NextReplenishment,
        WindowSemantics::AnchoredPeriod => BoundaryKind::FullReset,
        WindowSemantics::CalendarCycle => BoundaryKind::BillingBoundary,
        WindowSemantics::Unknown => BoundaryKind::Unknown,
    }
}

/// A stable, readable key for a window category, used inside derived identities.
pub(crate) fn category_key(category: QuotaCategory) -> &'static str {
    match category {
        QuotaCategory::Session => "session",
        QuotaCategory::Weekly => "weekly",
        QuotaCategory::Monthly => "monthly",
        QuotaCategory::Daily => "daily",
        QuotaCategory::Custom => "custom",
    }
}

/// A stable pool identity for one provider and one locally resolved account.
pub(crate) fn pool_id(provider: ProviderId, seed: &str) -> QuotaPoolId {
    let text = bounded(&format!("{}-{seed}", provider.as_str()), MAX_ID_LEN);
    QuotaPoolId::new(text).unwrap_or_else(|_| QuotaPoolId::generate())
}

/// A stable window identity for one pool, scope, metric, and period definition.
pub(crate) fn window_id(
    provider: ProviderId,
    category: QuotaCategory,
    pool: &QuotaPoolId,
    resource: &str,
    bucket: Option<&str>,
) -> QuotaWindowId {
    let mut text = format!(
        "{}:{}:{}:{}",
        provider.as_str(),
        category_key(category),
        pool.as_str(),
        resource
    );
    if let Some(bucket) = bucket {
        text.push(':');
        text.push_str(bucket);
    }
    let text = bounded(&text, MAX_ID_LEN);
    QuotaWindowId::new(text).unwrap_or_else(|_| QuotaWindowId::generate())
}

/// A resource identifier that always satisfies the domain invariants.
pub(crate) fn resource_id(text: &str) -> ResourceId {
    ResourceId::new(identifier(text)).unwrap_or_else(|_| ResourceId::generate())
}

/// A display label trimmed to the domain's scope-label budget.
pub(crate) fn label(text: &str) -> String {
    let trimmed = text.trim();
    let budget = quota_domain::quota::scope::MAX_SCOPE_LABEL_LEN;
    let shortened: String = trimmed.chars().take(budget).collect();
    if shortened.is_empty() {
        return "Usage".to_owned();
    }
    shortened
}

/// A lower-case, punctuation-free form of provider text, never empty.
pub(crate) fn identifier(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            sanitized.push(character.to_ascii_lowercase());
        } else if !sanitized.ends_with('-') {
            sanitized.push('-');
        }
    }
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        return "unknown".to_owned();
    }
    bounded(trimmed, MAX_ID_LEN)
}

/// A masked form of an address, so no label carries the full address.
pub(crate) fn masked_address(text: &str) -> String {
    let trimmed = text.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return label(trimmed);
    };
    let Some(first) = local.chars().next() else {
        return label(domain);
    };
    label(&format!("{first}***@{domain}"))
}

/// Shortens text to a character budget, keeping it unique with a text hash.
///
/// The hash is a plain FNV-1a of the full text, so the shortened form is stable
/// across releases and process runs.
fn bounded(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max.saturating_sub(17)).collect();
    format!("{head}-{:016x}", fnv1a(text))
}

/// A stable 64-bit FNV-1a hash of the text.
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A scope for a provider-reported resource, or the typed failure when the
/// domain cannot represent it.
pub(crate) fn scope(resource: &str, resource_label: &str) -> Result<QuotaScope, ProviderError> {
    QuotaScope::new(resource_id(resource), label(resource_label)).map_err(|_| {
        ProviderError::InvalidData {
            detail: SCOPE_DETAIL.to_owned(),
        }
    })
}

/// A percentage measurement, or the issue that explains why it is unusable.
pub(crate) fn percentage(
    used: f64,
    precision: DecimalPrecision,
    field: &str,
) -> Result<Measurement, QuotaIssue> {
    if !used.is_finite() {
        return Err(QuotaIssue::NonFiniteValue {
            field: field.to_owned(),
        });
    }
    if used < 0.0 {
        return Err(QuotaIssue::NegativeUsage { value: used });
    }
    PercentageMeasurement::from_used_percent(used, precision)
        .map(Measurement::Percentage)
        .map_err(|_| QuotaIssue::NonFiniteValue {
            field: field.to_owned(),
        })
}

/// Everything one provider mapping produced from a payload.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedUsage {
    /// The decoded windows, in a stable order.
    pub windows: Vec<QuotaWindow>,
    /// Identities of the windows the provider was expected to report and did not.
    pub expected_but_missing: Vec<QuotaWindowId>,
    /// A plan label, when the payload carried one.
    pub plan_label: Option<String>,
    /// A principal label, when the payload carried one.
    pub principal_label: Option<String>,
}

impl DecodedUsage {
    /// An empty reading.
    #[must_use]
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            expected_but_missing: Vec::new(),
            plan_label: None,
            principal_label: None,
        }
    }

    /// Whether every expected window arrived.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.expected_but_missing.is_empty()
    }

    /// Builds the outcome the scheduler consumes.
    #[must_use]
    pub fn into_outcome(self, identity: VerifiedIdentity) -> FetchOutcome {
        let read = QuotaRead {
            identity,
            windows: self.windows,
            expected_but_missing: self.expected_but_missing.clone(),
            debug_metadata: None,
        };
        if self.is_complete() {
            FetchOutcome::Complete(read)
        } else {
            FetchOutcome::Partial {
                read,
                detail: "the provider did not report every expected window".to_owned(),
            }
        }
    }

    /// Adds a window and records it when the reading is only partial.
    pub(crate) fn push(&mut self, window: QuotaWindow) {
        if matches!(
            window.measurement,
            Measurement::Unavailable(UnavailableReason::NotReported)
        ) {
            self.expected_but_missing.push(window.id.clone());
        }
        self.windows.push(window);
    }

    /// Records a window the provider was expected to report and did not.
    pub(crate) fn note_missing(&mut self, window: &QuotaWindow) {
        self.expected_but_missing.push(window.id.clone());
    }
}

impl Default for DecodedUsage {
    fn default() -> Self {
        Self::new()
    }
}

/// The description of one window before its measurement is known.
pub(crate) struct WindowDraft<'a> {
    /// The provider that owns this window.
    pub(crate) provider: ProviderId,
    /// The allowance owner.
    pub(crate) pool_id: &'a QuotaPoolId,
    /// The period category.
    pub(crate) category: QuotaCategory,
    /// The raw metered-resource text.
    pub(crate) resource: &'a str,
    /// The label shown beside the resource.
    pub(crate) resource_label: &'a str,
    /// The provider's own bucket identifier, when it reported one.
    pub(crate) bucket_id: Option<&'a str>,
    /// Whether this is included quota, a cap, or a balance.
    pub(crate) metric_role: MetricRole,
    /// What the period means.
    pub(crate) semantics: WindowSemantics,
    /// The reported duration, when the source gave one.
    pub(crate) duration_seconds: Option<i64>,
    /// When this process received the payload.
    pub(crate) received_at: DateTime<Utc>,
}

impl WindowDraft<'_> {
    /// Builds the window, filling everything the source did not report.
    ///
    /// A window that carries an issue, or no usable number, is marked partial so
    /// the account is never presented as freshly complete.
    pub(crate) fn build(
        &self,
        measurement: Measurement,
        boundary_at: Option<DateTime<Utc>>,
        issues: Vec<QuotaIssue>,
    ) -> Result<QuotaWindow, ProviderError> {
        let completeness =
            if issues.is_empty() && !matches!(measurement, Measurement::Unavailable(_)) {
                Completeness::Complete
            } else {
                Completeness::Partial
            };
        Ok(QuotaWindow {
            id: window_id(
                self.provider,
                self.category,
                self.pool_id,
                self.resource,
                self.bucket_id,
            ),
            provider_bucket_id: self.bucket_id.map(str::to_owned),
            pool_id: self.pool_id.clone(),
            scope: scope(self.resource, self.resource_label)?,
            category: self.category,
            semantics: self.semantics,
            duration: self.duration_seconds.map(Duration::seconds),
            metric_role: self.metric_role,
            enforcement: Enforcement::Unknown,
            measurement,
            period_started_at: None,
            boundary: boundary_at.map(|at| Boundary {
                at,
                kind: boundary_kind(self.semantics),
            }),
            observed_at: None,
            received_at: self.received_at,
            valid_until: None,
            source: SourceKind::ObservedWebEndpoint,
            completeness,
            definition_version: DefinitionVersion::INITIAL,
            issues,
        })
    }

    /// Builds a window the provider was expected to report and did not.
    pub(crate) fn reported_missing(&self) -> Result<QuotaWindow, ProviderError> {
        self.build(
            Measurement::Unavailable(UnavailableReason::NotReported),
            None,
            Vec::new(),
        )
    }

    /// Builds a window whose number the provider sent unusably.
    pub(crate) fn invalid(&self, issues: Vec<QuotaIssue>) -> Result<QuotaWindow, ProviderError> {
        self.build(
            Measurement::Unavailable(UnavailableReason::InvalidResponse),
            None,
            issues,
        )
    }

    /// Builds the window that means "this provider cannot express that window".
    pub(crate) fn unsupported(&self) -> Result<QuotaWindow, ProviderError> {
        self.build(
            Measurement::Unavailable(UnavailableReason::Unsupported),
            None,
            Vec::new(),
        )
    }
}

/// The typed failure used when a payload carries no usable identity.
pub(crate) fn missing_identity() -> ProviderError {
    ProviderError::InvalidData {
        detail: IDENTITY_DETAIL.to_owned(),
    }
}

/// Checks a live credential against the binding a read was requested for.
///
/// A reading is refused when the credential no longer belongs to the verified
/// account, which is what stops a refreshed login from answering for the wrong
/// account.
pub(crate) fn ensure_binding(
    binding: &ConnectionBinding,
    provider: ProviderId,
    principal: Option<&str>,
    profile_label: Option<&str>,
) -> Result<(), ProviderError> {
    if binding.provider_id != provider {
        return Err(ProviderError::InvalidData {
            detail: "the binding names another provider".to_owned(),
        });
    }
    match (binding.principal_id.as_ref(), principal) {
        (Some(verified), Some(current)) if verified.as_str() != current => {
            return Err(ProviderError::Authentication);
        }
        (Some(_), None) => return Err(ProviderError::Authentication),
        _ => {}
    }
    if let (Some(verified), Some(current)) = (binding.profile_label.as_deref(), profile_label)
        && verified != current
    {
        return Err(ProviderError::InvalidData {
            detail: "the binding names another credential profile".to_owned(),
        });
    }
    Ok(())
}
