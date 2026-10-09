//! The windows each fixture profile reports.
//!
//! Every value is derived from the profile alone, so a fixture reading never
//! depends on the network, on the absolute clock value, or on another account's
//! state.

use chrono::{DateTime, Duration, Utc};
use quota_core::ports::{ConnectionBinding, ProviderError, ReadContext};
#[cfg(feature = "test-fixtures")]
use quota_core::{accounts::AccountRegistry, ports::AccountRepository};
use quota_domain::account::VerifiedIdentity;
use quota_domain::ids::{
    ConnectionAttemptId, ConnectionId, DefinitionVersion, ProviderPrincipalId, QuotaPoolId,
    QuotaWindowId, ResourceId,
};
use quota_domain::provider::ProviderId;
use quota_domain::quota::measurement::{Measurement, PercentageMeasurement, QuantityMeasurement};
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::units::{DecimalPrecision, QuotaUnit, UnitSymbol};
use quota_domain::quota::window::QuotaCategory::{Monthly, Session, Weekly};
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
    specs(profile)
        .iter()
        .map(|spec| window(pool, spec, received_at))
        .collect()
}

/// One reported window, before it is built.
struct WindowSpec {
    category: QuotaCategory,
    bucket: &'static str,
    measurement: MeasurementSpec,
    /// Minutes from the receipt instant to the boundary. A negative value is a
    /// boundary that has already passed.
    boundary_minutes: Option<i64>,
    semantics: WindowSemantics,
}

/// How a fixture window measures its allowance.
enum MeasurementSpec {
    /// A used percentage at the fixture's fixed precision.
    UsedPercent(f64),
    /// A count against no denominator, so no percentage can be derived.
    NativeUnits { used: f64, remaining: f64 },
    /// No ceiling at all.
    Unlimited,
}

/// A rolling-period percentage window.
const fn rolling(
    category: QuotaCategory,
    bucket: &'static str,
    used: f64,
    minutes: i64,
) -> WindowSpec {
    WindowSpec {
        category,
        bucket,
        measurement: MeasurementSpec::UsedPercent(used),
        boundary_minutes: Some(minutes),
        semantics: WindowSemantics::RollingPeriod,
    }
}

/// A calendar-cycle percentage window.
const fn calendar(
    category: QuotaCategory,
    bucket: &'static str,
    used: f64,
    minutes: i64,
) -> WindowSpec {
    WindowSpec {
        category,
        bucket,
        measurement: MeasurementSpec::UsedPercent(used),
        boundary_minutes: Some(minutes),
        semantics: WindowSemantics::CalendarCycle,
    }
}

/// The windows one profile reports.
///
/// The first five profiles are the isolated reading shapes the tests drive
/// directly. The last five give the `sample-data` build its remaining accounts.
fn specs(profile: FixtureProfile) -> Vec<WindowSpec> {
    const HOUR: i64 = 60;
    const DAY: i64 = 24 * HOUR;
    match profile {
        FixtureProfile::Healthy => vec![
            rolling(Session, "session", 28.0, 3 * HOUR),
            rolling(Weekly, "weekly", 64.0, 4 * DAY),
            calendar(Monthly, "monthly", 8.0, 21 * DAY),
        ],
        FixtureProfile::MonthlyExhausted => vec![
            rolling(Session, "session", 5.0, HOUR),
            calendar(Monthly, "monthly", 100.0, 9 * DAY),
        ],
        FixtureProfile::StaleBoundary => vec![rolling(Session, "session", 96.0, -30)],
        FixtureProfile::NativeUnitsOnly => vec![WindowSpec {
            category: QuotaCategory::Custom,
            bucket: "native-units",
            measurement: MeasurementSpec::NativeUnits {
                used: 40.0,
                remaining: 340.0,
            },
            boundary_minutes: None,
            semantics: WindowSemantics::Unknown,
        }],
        FixtureProfile::Unlimited => vec![WindowSpec {
            category: Session,
            bucket: "session",
            measurement: MeasurementSpec::Unlimited,
            boundary_minutes: None,
            semantics: WindowSemantics::RollingPeriod,
        }],
        // The five sample profiles. Each reading is a fixed percentage, so the
        // ten sample accounts stay distinguishable and offline.
        FixtureProfile::Personal => vec![
            rolling(Session, "session", 28.0, 2 * HOUR),
            rolling(Weekly, "weekly", 59.0, 4 * DAY),
        ],
        FixtureProfile::Work => vec![
            rolling(Session, "session", 91.0, 40),
            rolling(Weekly, "weekly", 48.0, 3 * DAY),
        ],
        FixtureProfile::Experiments => vec![
            rolling(Session, "session", 12.0, 4 * HOUR),
            rolling(Weekly, "weekly", 18.0, 6 * DAY),
        ],
        FixtureProfile::ClientProject => vec![
            rolling(Session, "session", 68.0, HOUR),
            rolling(Weekly, "weekly", 33.0, 2 * DAY),
            calendar(Monthly, "monthly", 100.0, 12 * DAY),
        ],
        FixtureProfile::Studio => vec![
            rolling(Session, "session", 3.0, 3 * HOUR),
            rolling(Weekly, "weekly", 8.0, 5 * DAY),
        ],
    }
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
    spec: &WindowSpec,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let bucket = spec.bucket;
    let resource = ResourceId::new(bucket).map_err(|_| fixture_error())?;
    let scope =
        QuotaScope::new(resource, format!("Fixture {bucket}")).map_err(|_| fixture_error())?;
    let id = QuotaWindowId::new(format!("fixture:{}:{}", pool.as_str(), bucket))
        .map_err(|_| fixture_error())?;
    let measurement = match spec.measurement {
        MeasurementSpec::UsedPercent(used) => Measurement::Percentage(percent(used)?),
        MeasurementSpec::NativeUnits { used, remaining } => {
            Measurement::Quantity(QuantityMeasurement {
                unit: QuotaUnit::Custom(
                    UnitSymbol::new("fixture-points").map_err(|_| fixture_error())?,
                ),
                precision: precision(0)?,
                used: Some(used),
                remaining: Some(remaining),
                // No denominator, so this window never yields a percentage.
                limit: None,
            })
        }
        MeasurementSpec::Unlimited => Measurement::Unlimited,
    };
    let boundary = spec
        .boundary_minutes
        .map(|minutes| received_at + Duration::minutes(minutes))
        .map(|at| Boundary {
            at,
            kind: match spec.semantics {
                WindowSemantics::RollingPeriod => BoundaryKind::NextReplenishment,
                WindowSemantics::CalendarCycle => BoundaryKind::BillingBoundary,
                _ => BoundaryKind::Unknown,
            },
        });
    Ok(QuotaWindow {
        id,
        provider_bucket_id: Some(bucket.to_owned()),
        pool_id: pool.clone(),
        scope,
        category: spec.category,
        semantics: spec.semantics,
        duration: None,
        metric_role: MetricRole::IncludedAllowance,
        enforcement: Enforcement::Hard,
        measurement,
        period_started_at: None,
        boundary,
        observed_at: None,
        received_at: Utc::now(),
        valid_until: None,
        source: SourceKind::LocalCapture,
        completeness: Completeness::Complete,
        definition_version: DefinitionVersion::INITIAL,
        issues: Vec::new(),
    })
}

/// The display name of each sample account, in [`FixtureProfile::ALL`] order.
///
/// The wireframe-shaped profiles take the wireframe workspace names, and the
/// five test profiles take neutral workspace labels that match their reading.
/// Every name is fictional.
#[cfg(feature = "test-fixtures")]
const SAMPLE_ROLES: [&str; 10] = [
    "Home",
    "Acme",
    "Lab",
    "Sandbox",
    "Review",
    "Personal",
    "Work",
    "Experiments",
    "Client project",
    "Studio",
];

/// Persists and registers one account per fixture profile.
///
/// Every account goes through the caller's [`AccountRepository`] and
/// [`AccountRegistry`], exactly as a connection command does, so the seeded
/// state has the shape a real connection produces. One account per profile
/// means one quota pool per account, so ten rows carry ten distinct readings.
///
/// Seeding runs only into an empty registry, so a second launch reuses the ten
/// accounts it already persisted instead of adding ten more.
///
/// # Errors
///
/// Returns the failing phase and its diagnostic code.
#[cfg(feature = "test-fixtures")]
pub async fn seed_sample_accounts(
    adapter: &crate::fixture::FixtureAdapter,
    accounts: &std::sync::Arc<dyn AccountRepository>,
    registry: &mut AccountRegistry,
) -> Result<(), String> {
    if !registry.is_empty() {
        return Ok(());
    }
    let now = Utc::now();
    for (index, profile) in FixtureProfile::ALL.into_iter().enumerate() {
        let role = SAMPLE_ROLES.get(index).copied().unwrap_or("Sample");
        let seeded = sample_account(adapter, profile, role, now).await?;
        let new_account = quota_core::accounts::NewAccount {
            account_id: seeded.account_id.clone(),
            stored: seeded.clone(),
            profile_label: seeded.connection.profile_label.clone(),
            monitoring_enabled: true,
        };
        accounts
            .upsert_account(seeded)
            .await
            .map_err(|error| format!("sample_persist:{}", error.owner))?;
        registry
            .register(new_account)
            .map_err(|_| "sample_register_failed".to_owned())?;
    }
    Ok(())
}

/// Reads one profile and projects it into the stored-account shape.
#[cfg(feature = "test-fixtures")]
async fn sample_account(
    adapter: &crate::fixture::FixtureAdapter,
    profile: FixtureProfile,
    role: &str,
    now: DateTime<Utc>,
) -> Result<quota_core::ports::StoredAccount, String> {
    use quota_core::ports::ProviderAdapter as _;
    use quota_domain::account::{AccountCardinality, ConnectionState, ConnectionSummary};
    use quota_domain::account::{CredentialOwnership, FetchState};
    use quota_domain::ids::AccountId;

    let binding = fixture_binding(profile);
    let outcome = adapter
        .read_quota(&binding, fixture_context())
        .await
        .map_err(|error| format!("sample_read:{}", error.diagnostic_code()))?;
    let read = outcome
        .read()
        .cloned()
        .ok_or_else(|| "sample_read_unavailable".to_owned())?;
    let mut identity = read.identity;
    identity.workspace_label = Some(role.to_owned());
    Ok(quota_core::ports::StoredAccount {
        account_id: AccountId::generate(),
        connection: ConnectionSummary {
            id: ConnectionId::generate(),
            provider_id: ProviderId::Fixture,
            credential_ownership: CredentialOwnership::ExternalClient,
            generation: 0,
            profile_label: binding.profile_label,
            cardinality: AccountCardinality::Independent,
            state: ConnectionState::Connected,
            principal_id: binding.principal_id,
            workspace_id: None,
            entitlement_id: None,
        },
        nickname: role.to_owned(),
        connection_ordinal: 0,
        monitoring_enabled: true,
        connection_state: ConnectionState::Connected,
        fetch_state: FetchState::Idle,
        last_attempt_at: Some(now),
        last_success_at: Some(now),
        next_attempt_at: Some(now + Duration::seconds(300)),
        identity: Some(identity),
        windows: read.windows,
        expected_but_missing_window_ids: read.expected_but_missing,
        balance: None,
        show_key_limit: false,
        group: None,
    })
}
