/**
 * The application version, as the build stamps it.
 *
 * Vite replaces `__QUOTA_VERSION__` with the `version` in `package.json`, which
 * `cargo xtask check-release` keeps equal to the Tauri configuration's. It must
 * not be typed in by hand: after an update the window has to say the new version.
 */
declare const __QUOTA_VERSION__: string;

/** The version this build of Quota reports. */
export const APP_VERSION: string = __QUOTA_VERSION__;
