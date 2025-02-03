import pg from "pg";
import { keys } from "./keys";

const { Pool } = pg;

const db = new Pool({
  connectionString: keys().DATABASE_URL,
});

export async function addUser(clerkId: string) {
  await db.query(`INSERT INTO users (clerk_id, created_at) VALUES ($1, $2)`, [
    clerkId,
    new Date(),
  ]);
}

export async function deleteUser(clerkId: string) {
  await db.query(`DELETE FROM users WHERE clerk_id = $1`, [clerkId]);
}

// export async function setUserTelegramId(clerkId: string, telegramId: string) {
//   await db.query(
//     `UPDATE users SET telegram_id = $1, updated_at = $2 WHERE clerk_id = $3`,
//     [telegramId, new Date(), clerkId]
//   );
// }

export async function getUserByClerkId(clerkId: string) {
  const res = await db.query(
    `SELECT * FROM users WHERE clerk_id = $1 LIMIT 1`,
    [clerkId]
  );
  return res.rows.length ? res.rows[0] : null;
}

export async function getUserByToken(token: string) {
  const res = await db.query(
    `SELECT * FROM users WHERE public_token = $1 LIMIT 1`,
    [token]
  );
  return res.rows.length ? res.rows[0] : null;
}

// export async function createVisitor(fingerprint: string) {
//   await db.query(
//     `INSERT INTO visitors (fingerprint, created_at) VALUES ($1, $2)`,
//     [fingerprint, new Date()]
//   );
// }

export async function getOrCreateVisitor(fingerprint: string) {
  const result = await db.query(
    `
    INSERT INTO visitors (fingerprint, created_at)
    VALUES ($1, $2)
    ON CONFLICT (fingerprint) DO NOTHING
    RETURNING *;
    `,
    [fingerprint, new Date()]
  );

  // If a new visitor was inserted, return it
  if (result.rows.length > 0) {
    return result.rows[0];
  }

  // Otherwise, fetch the existing visitor
  const existingVisitor = await db.query(
    `SELECT * FROM visitors WHERE fingerprint = $1`,
    [fingerprint]
  );

  return existingVisitor.rows[0];
}

export async function addEvent(visitorId: string, data: any) {
  await db.query(
    `INSERT INTO events (visitor_id, data, created_at) VALUES ($1, $2, $3)`,
    [visitorId, JSON.stringify(data), new Date()]
  );
}

// export async function getTasks(clerkId: string) {
//   const query = await db.query(
//     `SELECT "value" FROM store WHERE prefix = $1 AND "key" = 'tasks'`,
//     [clerkId]
//   );

//   return query.rows.length ? query.rows[0].value.data : [];
// }
