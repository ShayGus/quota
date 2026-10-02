/**
 * Shared settings building blocks.
 *
 * One row layout, one switch, and one labelled select, so every panel states a
 * preference the same way.
 */
import { useId, type JSX, type ReactNode } from "react";

/** A panel's title, its introduction, and an optional action beside them. */
export function SettingsTitle({
  title,
  intro,
  action,
}: {
  readonly title: string;
  readonly intro: string;
  readonly action?: ReactNode;
}): JSX.Element {
  return (
    <div className="settings-title-row">
      <div>
        <h3>{title}</h3>
        <p className="settings-intro">{intro}</p>
      </div>
      {action}
    </div>
  );
}

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
        <span className="setting-label">{label}</span>
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
  disabled = false,
}: {
  readonly checked: boolean;
  readonly label: string;
  readonly onChange: (checked: boolean) => void;
  readonly disabled?: boolean;
}): JSX.Element {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
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
  readonly options: readonly (readonly [T, string, disabled?: boolean])[];
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
          const next = options.find(
            ([candidate]) => candidate === event.currentTarget.value,
          );
          if (next !== undefined) {
            onChange(next[0]);
          }
        }}
      >
        {options.map(([candidate, text, unsupported]) => (
          <option key={candidate} value={candidate} disabled={unsupported === true}>
            {text}
          </option>
        ))}
      </select>
    </>
  );
}

/**
 * The nickname input. With Hide account labels on, a nickname is an account
 * label like any other, so the field masks what is typed and says why; the
 * value is still saved as typed.
 */
export function NicknameField({
  id,
  value,
  hidden,
  onChange,
  hint,
}: {
  readonly id: string;
  readonly value: string;
  readonly hidden: boolean;
  readonly onChange: (value: string) => void;
  readonly hint?: string;
}): JSX.Element {
  const hintId = useId();
  const note = hidden ? "Hidden while Hide account labels is on." : hint;
  return (
    <>
      <label className="field-label" htmlFor={id}>
        Account nickname
      </label>
      <input
        type="text"
        id={id}
        className={hidden ? "masked" : undefined}
        autoComplete="off"
        maxLength={32}
        value={value}
        placeholder={hidden ? undefined : "For example: Personal"}
        required
        aria-describedby={note === undefined ? undefined : hintId}
        onChange={(event) => {
          onChange(event.currentTarget.value);
        }}
      />
      {note === undefined ? null : (
        <div className="form-hint" id={hintId}>
          {note}
        </div>
      )}
    </>
  );
}
