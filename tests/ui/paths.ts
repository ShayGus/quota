/** Where the bundled faked host is written and read. */
import { join } from "node:path";
import process from "node:process";

export const FAKE_BACKEND_DIRECTORY = join(
  process.cwd(),
  "node_modules",
  ".cache",
  "quota-ui",
);
export const FAKE_BACKEND_FILE = "fake-backend.js";
