import { getAccountByToken } from "@repo/database";
import fs from "fs";
import { NextRequest, NextResponse } from "next/server";
import path from "path";

export const GET = async (request: NextRequest) => {
  const searchParams = request.nextUrl.searchParams;
  const token = searchParams.get("token") || "";

  const user = await getAccountByToken(token);

  if (!user) {
    const errorJs = `console.error("Invalid token: Unauthorized request");`;
    return new NextResponse(Buffer.from(errorJs, "utf-8"), {
      status: 401,
      headers: {
        "content-type": "application/javascript",
      },
    });
  }

  const filePath = path.join(process.cwd(), "client", "dist", "scryer.iife.js");
  const proto = process.env.NODE_ENV === "production" ? "https" : "http";
  const host = request.headers.get("host");

  let js = fs.readFileSync(filePath, "utf-8");
  js = js.replace(/{{ API_URL }}/g, `${proto}://${host}`);
  js = js.replace(/{{ TOKEN }}/g, token);

  return new NextResponse(Buffer.from(js, "utf-8"), {
    status: 200,
    headers: {
      "content-type": "application/javascript",
      "cache-control": "no-store, no-cache, must-revalidate",
    },
  });
};

export const revalidate = 0;
