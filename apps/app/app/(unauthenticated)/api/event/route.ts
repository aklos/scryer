import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { enqueueEvent } from "@repo/queue";
import { parseEvent } from "./parse";
import { getAccountByToken } from "@repo/database";

export async function OPTIONS(request: NextRequest) {
  return new NextResponse(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": request.headers.get("origin") || "*",
      "Access-Control-Allow-Methods": "POST, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type, Authorization",
      "Access-Control-Allow-Credentials": "true",
    },
  });
}

export async function POST(request: NextRequest) {
  const origin = request.headers.get("origin") || "";
  const forwardedFor = request.headers.get("x-forwarded-for");
  const realIp = request.headers.get("x-real-ip");
  const ip = forwardedFor
    ? forwardedFor.split(",")[0].trim()
    : realIp
    ? realIp.trim()
    : undefined;
  const body = await request.json();
  const account = await getAccountByToken(body.token || "");

  if (!account || !account.allowed_origins.includes(origin)) {
    return NextResponse.json(
      { ok: false, error: "Invalid token/origin" },
      {
        status: 400,
        headers: {
          "Access-Control-Allow-Origin": origin,
          "Access-Control-Allow-Credentials": "true",
        },
      }
    );
  }

  delete body.token;

  try {
    const [data, validationError] = parseEvent(body);

    if (validationError) {
      return NextResponse.json(
        { ok: false, error: validationError },
        {
          status: 400,
          headers: {
            "Access-Control-Allow-Origin": origin,
            "Access-Control-Allow-Credentials": "true",
          },
        }
      );
    }

    data.origin = origin;
    data.ip = ip;

    await enqueueEvent(account.id, data);
  } catch (err: any) {
    return NextResponse.json(
      { ok: false, error: err.message },
      {
        status: 400,
        headers: {
          "Access-Control-Allow-Origin": origin,
          "Access-Control-Allow-Credentials": "true",
        },
      }
    );
  }

  return NextResponse.json(
    { ok: true },
    {
      headers: {
        "Access-Control-Allow-Origin": origin,
        "Access-Control-Allow-Credentials": "true",
      },
    }
  );
}

export const dynamic = "force-dynamic";
export const revalidate = 0;
