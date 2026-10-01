//! The windows each fixture profile reports.
//!
//! Every value is derived from the profile alone, so a fixture reading never
//! depends on the network, on the absolute clock value, or on another account's
//! state.

use chrono::{DateTime, Duration, Utc};
use quota_core::ports::{ConnectionBinding, ProviderError, ReadContext};
use quota_domain::account::VerifiedIdentity;
use quota_domain::ids::{
    ConnectionAttemptId, ConnectionId, DefinitionVersion, ProviderPrincipalId, QuotaPoolId,
    QuotaWindowId, ResourceId,
};
use quota_domain::provider::ProviderId;
use quota_domain::quota::measurement::{Measurement, PercentageMeasurement, QuantityMeasurement};
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::units::{DecimalPrecision, QuotaUnit, UnitSymbol};
use quota_domain::quota::window::{
    Boundary, BoundaryKind, Completeness, Enforcement, MetricRole, QuotaCategory, QuotaWindow,
    SourceKind, WindowSemantics,
};

use crate::fixture::FixtureProfile;

/// The detail text used when a fixture value cannot be represented.
const FIXTURE_DETAIL: &str = "a fixture value could not be represented";

/// The identity one fixture profile reports.
pub(crate) fn identity(profile: FixtureProfile) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: profile.nickname().to_owned(),
        workspace_label: None,
        plan_label: Some("fixture".to_owned()),
        source: SourceKind::LocalCapture,
    }
}

/// The typed failure used when a fixture value cannot be represented.
pub(crate) fn fixture_error() -> ProviderError {
    ProviderError::InvalidData {
        detail: FIXTURE_DETAIL.to_owned(),
    }
}

/// A read context for callers that drive the fixture directly.
#[must_use]
pub fn fixture_context() -> ReadContext {
    ReadContext {
        attempt_id: ConnectionAttemptId::generate(),
        deadline: None,
    }
}

/// A binding that addresses one fixture profile.
#[must_use]
pub fn fixture_binding(profile: FixtureProfile) -> ConnectionBinding {
    ConnectionBinding {
        connection_id: ConnectionId::generate(),
        generation: 1,
        provider_id: ProviderId::Fixture,
        principal_id: ProviderPrincipalId::new(profile.principal()).ok(),
        workspace_id: None,
        entitlement_id: None,
        profile_label: Some(profile.key().to_owned()),
    }
}

/// Every window one fixture profile reports.
pub(crate) fn windows(
    profile: FixtureProfile,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<Vec<QuotaWindow>, ProviderError> {
    let built = match profile {
        FixtureProfile::Healthy => vec![
            window(
                pool,
                QuotaCategory::Session,
                "session",
                Measurement::Percentage(percent(28.0)?),
                Some(received_at + Duration::hours(3)),
                WindowSemantics::RollingPeriod,
            )?,
            window(
                pool,
                QuotaCategory::Weekly,
                "weekly",
                Measurement::Percentage(percent(64.0)?),
                Some(received_at + Duration::days(4)),
                WindowSemantics::RollingPeriod,
            )?,
            window(
                pool,
                QuotaCategory::Monthly,
                "monthly",
                Measurement::Percentage(percent(8.0)?),
                Some(received_at + Duration::days(21)),
                WindowSemantics::CalendarCycle,
            )?,
        ],
        FixtureProfile::MonthlyExhausted => vec![
            window(
                pool,
                QuotaCategory::Session,
                "session",
                Measurement::Percentage(percent(5.0)?),
                Some(received_at + Duration::hours(1)),
                WindowSemantics::RollingPeriod,
            )?,
            window(
                pool,
                QuotaCategory::Monthly,
                "monthly",
                Measurement::Percentage(percent(100.0)?),
                Some(received_at + Duration::days(9)),
                WindowSemantics::CalendarCycle,
            )?,
        ],
        FixtureProfile::StaleBoundary => vec![window(
            pool,
            QuotaCategory::Session,
            "session",
            Measurement::Percentage(percent(96.0)?),
            Some(received_at - Duration::minutes(30)),
            WindowSemantics::RollingPeriod,
        )?],
        FixtureProfile::NativeUnitsOnly => vec![window(
            pool,
            QuotaCategory::Custom,
            "native-units",
            Measurement::Quantity(QuantityMeasurement {
                unit: QuotaUnit::Custom(
                    UnitSymbol::new("fixture-points").map_err(|_| fixture_error())?,
                ),
                precision: precision(0)?,
                used: Some(40.0),
                remaining: Some(340.0),
                // No denominator, so this window never yields a percentage.
                limit: None,
            }),
            None,
            WindowSemantics::Unknown,
        )?],
        FixtureProfile::Unlimited => vec![window(
            pool,
            QuotaCategory::Session,
            "session",
            Measurement::Unlimited,
            None,
            WindowSemantics::RollingPeriod,
        )?],
    };
    Ok(built)
}

/// A percentage measurement from a provider-reported used value.
fn percent(used: f64) -> Result<PercentageMeasurement, ProviderError> {
    PercentageMeasurement::from_used_percent(used, precision(1)?).map_err(|_| fixture_error())
}

/// A decimal precision, or the typed failure for an unrepresentable one.
fn precision(places: u8) -> Result<DecimalPrecision, ProviderError> {
    DecimalPrecision::new(places).map_err(|_| fixture_error())
}

/// One fixture window with the fixed shape every profile shares.
fn window(
    pool: &QuotaPoolId,
    category: QuotaCategory,
    bucket: &str,
    measurement: Measurement,
    boundary_at: Option<DateTime<Utc>>,
    semantics: WindowSemantics,
) -> Result<QuotaWindow, ProviderError> {
    let resource = ResourceId::new(bucket).map_err(|_| fixture_error())?;
    let scope =
        QuotaScope::new(resource, format!("Fixture {bucket}")).map_err(|_| fixture_error())?;
    let id = QuotaWindowId::new(format!("fixture:{}:{}", pool.as_str(), bucket))
        .map_err(|_| fixture_error())?;
    Ok(QuotaWindow {
        id,
        provider_bucket_id: Some(bucket.to_owned()),
        pool_id: pool.clone(),
        scope,
        category,
        semantics,
        duration: None,
        metric_role: MetricRole::IncludedAllowance,
        enforcement: Enforcement::Hard,
        measurement,
        period_started_at: None,
        boundary: boundary_at.map(|at| Boundary {
            at,
            kind: match semantics {
                WindowSemantics::RollingPeriod => BoundaryKind::NextReplenishment,
                WindowSemantics::CalendarCycle => BoundaryKind::BillingBoundary,
                _ => BoundaryKind::Unknown,
            },
        }),
        observed_at: None,
        received_at: Utc::now(),
        valid_until: None,
        source: SourceKind::LocalCapture,
        completeness: Completeness::Complete,
        definition_version: DefinitionVersion::INITIAL,
        issues: Vec::new(),
    })
}
