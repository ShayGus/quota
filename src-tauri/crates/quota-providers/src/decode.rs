//! The decoding rules every provider mapping shares.
//!
//! A provider mapping only turns its own wire payload into window drafts. The
//! decisions that must not drift between providers live here: how a window
//! identity is derived, which measurement stands for a missing or unusable
//! number, what a reported boundary means, and how an unverified account is
//! labelled.
//!
//! Two neighbours hold the leaf rules: [`values`] reads the numbers, instants,
//! and percentages, and [`identity`] derives identities and safe labels.

pub(crate) mod base64;
mod identity;
pub(crate) mod jwt;
mod values;

use chrono::{DateTime, Duration, Utc};
use quota_core::ports::{ConnectionBinding, FetchOutcome, ProviderError, QuotaRead};
use quota_domain::account::VerifiedIdentity;
use quota_domain::ids::{DefinitionVersion, QuotaPoolId, QuotaWindowId};
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_domain::quota::window::{
    Boundary, BoundaryKind, Completeness, Enforcement, MetricRole, QuotaCategory, QuotaWindow,
    SourceKind, WindowSemantics,
};

pub(crate) use identity::{fingerprint, identifier, masked_address, pool_id, scope, window_id};
pub(crate) use values::{
    Numberish, counted, percentage, reset_after, reset_epoch, reset_instant, scaled_seconds,
};

/// The detail text used when a payload carries no usable identity.
const IDENTITY_DETAIL: &str = "the payload carried no usable account identity";

/// The boundary kind a window's own semantics prove.
///
/// Only a bucket whose name states incremental replenishment earns that label.
/// Every other reported time is an unspecified provider event, because these
/// sources do not say what their boundaries mean.
pub(crate) const fn boundary_kind(semantics: WindowSemantics) -> BoundaryKind {
    match semantics {
        WindowSemantics::RollingPeriod => BoundaryKind::NextReplenishment,
        WindowSemantics::AnchoredPeriod => BoundaryKind::FullReset,
        WindowSemantics::CalendarCycle => BoundaryKind::BillingBoundary,
        WindowSemantics::Unknown => BoundaryKind::Unknown,
    }
}

/// Everything one provider mapping produced from a payload.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecodedUsage {
    /// The decoded windows, in a stable order.
    pub windows: Vec<QuotaWindow>,
    /// Identities of the windows the provider was expected to report and did not.
    pub expected_but_missing: Vec<QuotaWindowId>,
    /// A plan label, when the payload carried one.
    pub plan_label: Option<String>,
    /// A principal label, when the payload carried one.
    pub principal_label: Option<String>,
    /// What the payload said about a prepaid balance, when it had one.
    pub balance: Option<quota_domain::balance::BalanceReading>,
}

impl DecodedUsage {
    /// An empty reading.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            windows: Vec::new(),
            expected_but_missing: Vec::new(),
            plan_label: None,
            principal_label: None,
            balance: None,
        }
    }

    /// Whether every expected window arrived.
    #[must_use]
    pub(crate) fn is_complete(&self) -> bool {
        self.expected_but_missing.is_empty()
    }

    /// Builds the outcome the scheduler consumes.
    #[must_use]
    pub(crate) fn into_outcome(self, identity: VerifiedIdentity) -> FetchOutcome {
        let complete = self.is_complete();
        let read = QuotaRead {
            identity,
            windows: self.windows,
            expected_but_missing: self.expected_but_missing,
            balance: self.balance,
            debug_metadata: None,
        };
        if complete {
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

/// Deserializes a field that a provider sends as `null` when it has nothing to
/// report, as its empty value.
///
/// `#[serde(default)]` covers a missing field only; an explicit `null` for a
/// list or a map is otherwise a type error that rejects the whole payload. The
/// usage endpoints are undocumented and do send `null` for empty collections.
pub(crate) fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + serde::Deserialize<'de>,
{
    Ok(<Option<T> as serde::Deserialize>::deserialize(deserializer)?.unwrap_or_default())
}
