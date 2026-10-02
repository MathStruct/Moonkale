// The server's state store (Milestone 18 phase 5), read by the suites as a second process: SQLite's
// table kv (t = table, k = key, v = the record's envelope {"v", "data"}). serve.sh points the
// server at MOONKALE_CONFIG_DIR=$E/cfg; run-all.sh clears the folder-host tables between suites.
import fs from "node:fs";
import { DatabaseSync } from "node:sqlite";

const ROOT = process.env.M1_ROOT;
export const STATE = `${process.env.MOONKALE_CONFIG_DIR ?? ROOT + "/../cfg"}/state.sqlite`;

// A key's parts (moonkale_state::Key): strings end in 00 01 (00 inside is 00 FF); whatever is left
// (a u64 or u128, big-endian) comes back as `rest`.
export function keyParts(k) {
  const parts = [];
  let i = 0, cur = [];
  while (i < k.length) {
    if (k[i] === 0 && k[i + 1] === 1) { parts.push(Buffer.from(cur).toString("utf8")); cur = []; i += 2; continue; }
    if (k[i] === 0 && k[i + 1] === 0xff) { cur.push(0); i += 2; continue; }
    if (k[i] === 0) break;
    cur.push(k[i]); i++;
  }
  // Bytes after the last string that did not end in a terminator are a number part.
  const consumed = parts.reduce((n, p) => n + Buffer.byteLength(p) + 2, 0);
  return { parts, rest: k.subarray(consumed) };
}

// [{ parts, rest, data }] of one table, in key order.
export function rows(table) {
  if (!fs.existsSync(STATE)) return [];
  const db = new DatabaseSync(STATE, { readOnly: true });
  try {
    return db.prepare("SELECT k, v FROM kv WHERE t = ? ORDER BY k").all(table).map((r) => ({
      ...keyParts(Buffer.from(r.k)),
      data: JSON.parse(Buffer.from(r.v).toString("utf8")).data,
    }));
  } finally { db.close(); }
}
