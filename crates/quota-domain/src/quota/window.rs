//! A quota window: one allowance for one scope over one period.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ids::{DefinitionVersion, QuotaPoolId, QuotaWindowId};
use crate::quota::issue::QuotaIssue;
use crate::quota::measurement::Measurement;
use crate::quota::scope::QuotaScope;

/// The period a window covers, as a projection category.
///
/// The three common UI columns are derived from this; they are not a fixed
/// three-field schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QuotaCategory {
    /// A short rolling session allowance, often five hours.
    Session,
    /// A weekly allowance.
    Weekly,
    /// A monthly allowance.
    Monthly,
    /// A daily allowance.
    Daily,
    /// A provider-defined period.
    Custom,
}

impl QuotaCategory {
    /// The comparison column this category projects into, if any.
    #[must_use]
    pub const fn column(self) -> Option<WindowColumn> {
        match self {
            Self::Session => Some(WindowColumn::Session),
            Self::Weekly => Some(WindowColumn::Weekly),
            Self::Monthly => Some(WindowColumn::Monthly),
            Self::Daily | Self::Custom => None,
        }
    }
}

/// One of the simultaneously visible comparison columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WindowColumn {
    /// The session column.
    Session,
    /// The weekly column.
    Weekly,
    /// The monthly column.
    Monthly,
}

/// What a duration actually means. A duration alone never decides this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WindowSemantics {
    /// A period anchored to a start time.
    AnchoredPeriod,
    /// A genuinely rolling window.
    RollingPeriod,
    /// A calendar billing cycle.
    CalendarCycle,
    /// The source did not say.
    Unknown,
}

/// What kind of change happens at a boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryKind {
    /// The whole allowance returns.
    FullReset,
    /// Part of the allowance returns, so "next replenishment" is the honest label.
    NextReplenishment,
    /// A billing boundary, not necessarily an allowance change.
    BillingBoundary,
    /// The source reported a time without saying what it means.
    Unknown,
}

/// A reported time at which the window's allowance changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Boundary {
    /// The boundary instant, in UTC.
    pub at: DateTime<Utc>,
    /// What the boundary means.
    pub kind: BoundaryKind,
}

/// What the number represents in the plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MetricRole {
    /// The allowance included in the subscription. The only finite role eligible
    /// for the least-remaining ranking.
    IncludedAllowance,
    /// A ceiling on extra spend. Not included quota.
    ExtraSpendCap,
    /// An informational balance. Not included quota.
    CreditBalance,
}

impl MetricRole {
    /// Whether this role may take part in a finite included-quota comparison.
    #[must_use]
    pub const fn is_included_allowance(self) -> bool {
        matches!(self, Self::IncludedAllowance)
    }
}

/// How strongly the provider enforces this limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    /// New work is refused.
    Hard,
    /// The provider warns but continues.
    Soft,
    /// The value is only reported.
    Informational,
    /// The source did not say.
    Unknown,
}

/// Where a reading came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A documented provider API.
    DocumentedApi,
    /// A documented command-line protocol.
    DocumentedCliProtocol,
    /// An observed provider web endpoint, not a public contract.
    ObservedWebEndpoint,
    /// A locally captured response.
    LocalCapture,
    /// Entered by the user.
    Manual,
}

/// Whether every applicable field of this window was reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    /// Every applicable field arrived.
    Complete,
    /// Some applicable fields are missing; known values stay visible.
    Partial,
}

/// One allowance, for one account, over one period.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct QuotaWindow {
    /// Stable identity of this pool, scope, metric and period definition.
    pub id: QuotaWindowId,
    /// The provider's own bucket identifier, when it reported one.
    pub provider_bucket_id: Option<String>,
    /// The allowance owner. Several accounts may share one pool.
    pub pool_id: QuotaPoolId,
    /// The metered resource.
    pub scope: QuotaScope,
    /// The period category this window projects into.
    pub category: QuotaCategory,
    /// What the period actually means.
    pub semantics: WindowSemantics,
    /// The reported duration, when the source gave one. Never decides semantics.
    pub duration: Option<Duration>,
    /// Whether this is included quota, an extra-spend cap, or a balance.
    pub metric_role: MetricRole,
    /// How strongly the provider enforces the limit.
    pub enforcement: Enforcement,
    /// The normalised reading.
    pub measurement: Measurement,
    /// When the current period started, when known.
    pub period_started_at: Option<DateTime<Utc>>,
    /// When the allowance changes, when known.
    pub boundary: Option<Boundary>,
    /// When the provider says it observed the value.
    pub observed_at: Option<DateTime<Utc>>,
    /// When this process received it.
    pub received_at: DateTime<Utc>,
    /// After this instant the reading is stale, when the source defines one.
    pub valid_until: Option<DateTime<Utc>>,
    /// Where the reading came from.
    pub source: SourceKind,
    /// Whether every applicable field arrived.
    pub completeness: Completeness,
    /// The provider's version of this window definition.
    pub definition_version: DefinitionVersion,
    /// Structured validation findings.
    pub issues: Vec<QuotaIssue>,
}

impl QuotaWindow {
    /// Whether the boundary has passed, so the current-period claim no longer holds.
    #[must_use]
    pub fn boundary_has_passed(&self, now: DateTime<Utc>) -> bool {
        self.boundary.is_some_and(|boundary| boundary.at <= now)
    }

    /// Whether the reading is past its freshness deadline.
    #[must_use]
    pub fn is_stale_at(&self, now: DateTime<Utc>) -> bool {
        self.valid_until.is_some_and(|deadline| deadline <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ResourceId;
    use crate::quota::measurement::{PercentageMeasurement, UnavailableReason};
    use crate::quota::units::DecimalPrecision;

    fn window(measurement: Measurement, boundary: Option<Boundary>) -> QuotaWindow {
        QuotaWindow {
            id: QuotaWindowId::new("w1").unwrap(),
            provider_bucket_id: None,
            pool_id: QuotaPoolId::new("p1").unwrap(),
            scope: QuotaScope::new(ResourceId::new("model_x").unwrap(), "Model X").unwrap(),
            category: QuotaCategory::Weekly,
            semantics: WindowSemantics::Unknown,
            duration: None,
            metric_role: MetricRole::IncludedAllowance,
            enforcement: Enforcement::Unknown,
            measurement,
            period_started_at: None,
            boundary,
            observed_at: None,
            received_at: DateTime::UNIX_EPOCH,
            valid_until: None,
            source: SourceKind::DocumentedApi,
            completeness: Completeness::Complete,
            definition_version: DefinitionVersion::INITIAL,
            issues: Vec::new(),
        }
    }

    fn percent(remaining: f64) -> Measurement {
        Measurement::Percentage(
            PercentageMeasurement::from_used_percent(
                100.0 - remaining,
                DecimalPrecision::new(0).unwrap(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn boundary_expiry_is_detected_without_refilling() {
        let now = DateTime::UNIX_EPOCH + Duration::hours(6);
        let due = Boundary {
            at: now,
            kind: BoundaryKind::FullReset,
        };
        assert!(window(percent(80.0), Some(due)).boundary_has_passed(now));
        assert!(!window(percent(80.0), Some(due)).boundary_has_passed(now - Duration::hours(1)));
        assert!(!window(percent(80.0), None).boundary_has_passed(now));
    }

    #[test]
    fn staleness_uses_the_source_freshness_deadline() {
        let now = DateTime::UNIX_EPOCH + Duration::hours(2);
        let mut value = window(percent(50.0), None);
        assert!(!value.is_stale_at(now));
        value.valid_until = Some(now - Duration::minutes(1));
        assert!(value.is_stale_at(now));
    }

    #[test]
    fn only_included_allowance_is_rankable() {
        assert!(MetricRole::IncludedAllowance.is_included_allowance());
        assert!(!MetricRole::ExtraSpendCap.is_included_allowance());
        assert!(!MetricRole::CreditBalance.is_included_allowance());
    }

    #[test]
    fn an_unavailable_window_carries_no_number() {
        let value = window(
            Measurement::Unavailable(UnavailableReason::NotReported),
            None,
        );
        assert!(!value.measurement.has_number());
    }
}
