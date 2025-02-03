import { NextRequest } from "next/server";
import { sendMessage } from "@repo/messaging";
import { getUserByClerkId } from "@repo/database";

export async function POST(request: NextRequest) {
  const secretKey = request.headers.get("x-secret-key");

  if (secretKey !== process.env.AGENT_API_KEY) {
    return new Response("Unauthorized", { status: 401 });
  }

  const { message, clerkId, uiElements } = await request.json();
  console.log(uiElements);

  const user = await getUserByClerkId(clerkId);

  if (user && user.telegram_id) {
    sendMessage(user.telegram_id, message);
  }

  return new Response("OK", { status: 200 });
}
