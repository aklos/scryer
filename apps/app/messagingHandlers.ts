import { messagingEvents, sendMessage, showTyping } from "@repo/messaging";
import { getUserByTelegramId, setUserTelegramId } from "@repo/database";

messagingEvents.on("start", async ({ clerkId, telegramId }) => {
  if (clerkId) {
    await setUserTelegramId(clerkId, telegramId);
  }
});

messagingEvents.on("message", async ({ telegramId, chatId, message }) => {
  const user = await getUserByTelegramId(telegramId);

  if (user) {
    showTyping(chatId);

    const apiUrl = `${process.env.AGENT_API_URL}/run-thread`;

    // const date = new Date();
    // date.setMinutes(date.getMinutes() - user.tz_offset);
    // const timestamp = date.toISOString();

    const response = await fetch(apiUrl, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-secret-key": process.env.AGENT_API_KEY || "",
      },
      body: JSON.stringify({
        clerk_id: user.clerk_id,
        tz_offset: user.tz_offset,
        onboarded: user.onboarded,
        message,
      }),
    });

    if (!response.ok) {
      sendMessage(chatId, "[ERROR] Couldn't connect to the API.");
    }

    const data = await response.json();
    sendMessage(chatId, data.response.content);
  }
});
