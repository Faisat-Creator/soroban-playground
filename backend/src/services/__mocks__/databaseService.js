// Stub for databaseService.js (sqlite3-based) — not used by the functional API tests
export default class DatabaseService {
  constructor() {}
  async connect() {}
  async close() {}
  async run() {
    return {};
  }
  async get() {
    return null;
  }
  async all() {
    return [];
  }
  async transaction(fn) {
    return fn(this);
  }
}
