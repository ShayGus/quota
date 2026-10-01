/**
 * Shared settings building blocks.
 *
 * One row layout, one switch, and one labelled select, so every panel states a
 * preference the same way.
 */
import { useId, type JSX } from "react";

/** One labelled setting row with its control. */
export function SettingRow({
  label,
  description,
  control,
}: {
  readonly label: string;
  readonly description: string;
  readonly control: JSX.Element;
}): JSX.Element {
  return (
    <div className="setting-row">
      <div>
        <span className="setting-row__label">{label}</span>
        <p>{description}</p>
      </div>
      {control}
    </div>
  );
}

/** An accessible switch. */
export function Switch({
  checked,
  label,
  onChange,
}: {
  readonly checked: boolean;
  readonly label: string;
  readonly onChange: (checked: boolean) => void;
}): JSX.Element {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => {
        onChange(!checked);
      }}
    />
  );
}

/** A labelled select over a closed set of options. */
export function Select<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  readonly label: string;
  readonly value: T;
  readonly options: readonly (readonly [T, string])[];
  readonly onChange: (value: T) => void;
}): JSX.Element {
  const id = useId();
  return (
    <>
      <label className="sr-only" htmlFor={id}>
        {label}
      </label>
      <select
        id={id}
        aria-label={label}
        value={value}
        onChange={(event) => {
          const next = options.find(([candidate]) => candidate === event.currentTarget.value);
          if (next !== undefined) {
            onChange(next[0]);
          }
        }}
      >
        {options.map(([candidate, text]) => (
          <option key={candidate} value={candidate}>
            {text}
          </option>
        ))}
      </select>
    </>
  );
}
