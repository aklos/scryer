import pg from "pg";
import { keys } from "./keys";
import { getCountryFromIP } from "@repo/geoip";

const { Pool } = pg;

const db = new Pool({
  connectionString: keys().DATABASE_URL,
});

export async function addAccount(
  clerkId: string,
  publicToken: string,
  secretKey: string
) {
  await db.query(
    `INSERT INTO accounts (clerk_id, public_token, secret_key, created_at) VALUES ($1, $2)`,
    [clerkId, publicToken, secretKey, new Date()]
  );
}

export async function deleteAccount(clerkId: string) {
  await db.query(`DELETE FROM accounts WHERE clerk_id = $1`, [clerkId]);
}

// export async function setUserTelegramId(clerkId: string, telegramId: string) {
//   await db.query(
//     `UPDATE users SET telegram_id = $1, updated_at = $2 WHERE clerk_id = $3`,
//     [telegramId, new Date(), clerkId]
//   );
// }

export async function getAccountByClerkId(clerkId: string) {
  const res = await db.query(
    `SELECT * FROM accounts WHERE clerk_id = $1 LIMIT 1`,
    [clerkId]
  );
  return res.rows.length ? res.rows[0] : null;
}

export async function getAccountByToken(token: string) {
  const res = await db.query(
    `SELECT * FROM accounts WHERE public_token = $1 LIMIT 1`,
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

export async function getOrCreateVisitor(
  accountId: string,
  fingerprint: string,
  ip: string | undefined
) {
  const existingVisitor = await db.query(
    `SELECT * FROM visitors WHERE fingerprint = $1`,
    [fingerprint]
  );

  if (existingVisitor.rows.length > 0) {
    return existingVisitor.rows[0];
  }

  let countryCode: string | undefined = undefined;

  try {
    const country = ip ? await getCountryFromIP(ip) : null;
    if (country?.country?.isoCode) {
      countryCode = country.country?.isoCode;
    }
  } catch (err) {
    // pass
    console.log(err);
  }

  const result = await db.query(
    `
    INSERT INTO visitors (account_id, fingerprint, country, created_at)
    VALUES ($1, $2, $3, $4)
    ON CONFLICT (fingerprint) DO NOTHING
    RETURNING *;
    `,
    [accountId, fingerprint, countryCode, new Date()]
  );

  // If a new visitor was inserted, return it
  if (result.rows.length > 0) {
    return result.rows[0];
  }

  return null;
}

export async function addEvent(visitorId: string, data: any) {
  await db.query(
    `INSERT INTO events (visitor_id, data, created_at) VALUES ($1, $2, $3)`,
    [visitorId, JSON.stringify(data), new Date()]
  );
}

export async function setVisitorLeadStatus(
  visitorId: string,
  leadStatus: "non_lead" | "lead" | "converted"
) {
  await db.query(`UPDATE visitors SET lead_status = $1 WHERE id = $2`, [
    leadStatus,
    visitorId,
  ]);
}

export async function setVisitorHashedEmail(
  visitorId: string,
  hashedEmail: string
) {
  await db.query(`UPDATE visitors SET hashed_email = $1 WHERE id = $2`, [
    hashedEmail,
    visitorId,
  ]);
}
// export async function getTasks(clerkId: string) {
//   const query = await db.query(
//     `SELECT "value" FROM store WHERE prefix = $1 AND "key" = 'tasks'`,
//     [clerkId]
//   );

//   return query.rows.length ? query.rows[0].value.data : [];
// }
