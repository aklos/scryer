"use client";

import { useUser } from "@repo/auth/client";
import { useEffect, useState } from "react";
import { RuntimeProvider } from "./components/runtime-provider";
import { Thread } from "./components/ui/thread";

const ChatComponent = ({ systemPrompt }) => {
  return (
    <div className="h-full">
      <RuntimeProvider>
        <Thread />
      </RuntimeProvider>
    </div>
  );
};

const Chat = () => {
  const [isLoading, setIsLoading] = useState(false);

  if (isLoading) {
    return (
      <div className="flex justify-center items-center h-full">
        <p className="text-xl">Loading...</p>
      </div>
    );
  }

  return <ChatComponent systemPrompt={""} />;
};

export default Chat;
