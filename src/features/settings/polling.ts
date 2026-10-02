import type {
  FixedIntervalPolicy,
  ProviderPollingPolicy,
} from "../../generated/bindings";

export function backgroundRefreshSeconds(policy: ProviderPollingPolicy): number {
  const strategy = policy.strategy;
  switch (strategy.kind) {
    case "fixed_interval":
      return strategy.settings.background_seconds;
    case "boundary_aware":
      return strategy.settings.base.background_seconds;
    case "adaptive":
      return strategy.settings.step_seconds;
    case "event_assisted":
      return strategy.settings.verification_seconds;
  }
}

export function withBackgroundRefresh(
  policy: ProviderPollingPolicy,
  seconds: number,
): ProviderPollingPolicy {
  const strategy = policy.strategy;
  const fixed = (base: FixedIntervalPolicy): FixedIntervalPolicy => {
    const interval = Math.max(seconds, base.minimum_seconds);
    return {
      ...base,
      visible_seconds: interval,
      background_seconds: interval,
      battery_saver_seconds: Math.max(base.battery_saver_seconds, interval),
    };
  };
  switch (strategy.kind) {
    case "fixed_interval":
      return { ...policy, strategy: { ...strategy, settings: fixed(strategy.settings) } };
    case "boundary_aware":
      return {
        ...policy,
        strategy: {
          ...strategy,
          settings: { ...strategy.settings, base: fixed(strategy.settings.base) },
        },
      };
    case "adaptive":
      return {
        ...policy,
        strategy: {
          ...strategy,
          settings: {
            ...strategy.settings,
            step_seconds: Math.min(
              strategy.settings.maximum_seconds,
              Math.max(seconds, strategy.settings.minimum_seconds),
            ),
          },
        },
      };
    case "event_assisted":
      return {
        ...policy,
        strategy: {
          ...strategy,
          settings: {
            ...strategy.settings,
            verification_seconds: Math.max(seconds, strategy.settings.minimum_seconds),
          },
        },
      };
  }
}
