import { NextRequest } from "next/server";
import { currentUser } from "@repo/auth/server";
import { getUserByClerkId } from "@repo/database";

export async function POST(request: NextRequest) {
  const user = await currentUser();

  if (!user) {
    return new Response("Not logged in.", { status: 403 });
  }

  const { message, tzOffset } = await request.json();

  let userData = await getUserByClerkId(user.id);
  // await updateTzOffset(user.id, tzOffset);

  // Define the API endpoint of your FastAPI server
  const apiUrl = `${process.env.AGENT_API_URL}/run-thread-stream`;

  // Send the POST request and get the streaming response
  const response = await fetch(apiUrl, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "x-secret-key": process.env.AGENT_API_KEY || "",
    },
    body: JSON.stringify({
      clerk_id: user.id,
      tz_offset: tzOffset,
      onboarded: userData.onboarded,
      message,
    }),
  });

  // Check if the response is OK
  if (!response.ok) {
    return new Response("Failed to connect to the API", { status: 500 });
  }

  // Stream the responses back to the client as SSE
  const reader = response.body?.getReader();
  const decoder = new TextDecoder("utf-8");

  return new Response(
    new ReadableStream({
      async start(controller) {
        if (!reader) {
          controller.close();
          return;
        }

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          const chunk = decoder.decode(value);
          if (
            chunk.trim() === `data: {"finished": true}` &&
            !userData.onboarded
          ) {
            // await setOnboarded(user.id, true);
            userData = await getUserByClerkId(user.id);
          }
          controller.enqueue(`${chunk}\n\n`); // SSE format: data: <message>\n\n
        }

        controller.close();
      },
    }),
    {
      headers: {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      },
    }
  );
}
