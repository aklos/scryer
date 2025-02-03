import TelegramBot from "node-telegram-bot-api";
import { keys } from "./keys";
import EventEmitter from "events";

export const messagingEvents = new EventEmitter();

declare global {
  var telegramBotInstance: TelegramBot | undefined;
}

const initializeTelegramBot = (): TelegramBot => {
  if (!global.telegramBotInstance) {
    console.log("Initializing Telegram bot...");
    global.telegramBotInstance = new TelegramBot(keys().TELEGRAM_BOT_TOKEN, {
      polling: true,
    });

    global.telegramBotInstance.onText(/\/start (.+)/, (msg, match) => {
      if (!match) return;
      const [, clerkId] = match;
      const telegramId = msg.from?.id;
      const chatId = msg.chat.id;
      messagingEvents.emit("start", { clerkId, telegramId, chatId });
    });

    global.telegramBotInstance.on("message", (msg) => {
      const telegramId = msg.from?.id;
      const chatId = msg.chat.id;
      messagingEvents.emit("message", {
        telegramId,
        chatId,
        message: msg.text,
      });
    });
  }
  return global.telegramBotInstance!;
};

// Initialize the bot during runtime setup
export const getTelegramBot = (): TelegramBot => {
  return initializeTelegramBot();
};

export const sendMessage = (chatId: number, message: string) => {
  const telegram = getTelegramBot();
  telegram.sendMessage(chatId, message);
};

export const showTyping = (chatId: number) => {
  const telegram = getTelegramBot();
  telegram.sendChatAction(chatId, "typing");
};
