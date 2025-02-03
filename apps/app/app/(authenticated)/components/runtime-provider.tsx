import {
  AssistantRuntimeProvider,
  ThreadAssistantContentPart,
  useLocalRuntime,
  type ChatModelAdapter,
} from "@assistant-ui/react";
import { useEffect, useState } from "react";

async function* backendApi({
  messages,
  abortSignal,
  config,
}: {
  messages: any;
  abortSignal?: AbortSignal;
  config?: any;
}) {
  const tzOffset = new Date().getTimezoneOffset();
  const response = await fetch("/api/chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      message: messages.at(-1).content[0].text,
      tzOffset,
    }),
    signal: abortSignal, // Pass the abort signal for cancellation support
  });

  if (!response.ok || !response.body) {
    throw new Error("Failed to connect to the backend");
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder("utf-8");
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });

    const lines = buffer.split("\r\n\r\n");
    buffer = lines.pop() || "";

    for (const line of lines) {
      if (line.trim().startsWith("data:")) {
        const data = line.trim().replace(/^data: /, "");
        if (data) {
          const parsed = JSON.parse(data);
          yield {
            choices: [
              {
                delta: { content: parsed.response },
              },
              {
                delta: { content: parsed.structured_response },
              },
              {
                delta: { content: JSON.stringify(parsed.tool_call) },
              },
            ],
          };
        }
      }
    }
  }
}

const MyModelAdapter: ChatModelAdapter = {
  async *run({ messages, abortSignal, config }) {
    const stream = await backendApi({ messages, abortSignal, config });

    let text = "";
    let dataText = "";
    let toolText = "";
    for await (const part of stream) {
      text += part.choices[0]?.delta?.content || "";
      dataText += part.choices[1]?.delta?.content || "";
      toolText += part.choices[2]?.delta?.content || "";

      let content: ThreadAssistantContentPart[] = [{ type: "text", text }];

      try {
        const data = JSON.parse(dataText);
        content.push({
          type: "ui",
          display: <pre className="hidden">{JSON.stringify(data)}</pre>,
        });
      } catch (err) {
        // pass
      }

      try {
        const data = JSON.parse(toolText);
        if (data.function.name === "ask_human") {
          content.push({
            type: "ui",
            display: <pre className="hidden">{data.function.arguments}</pre>,
          });
        }
        toolText = "";
      } catch (err) {
        // pass
      }

      yield {
        content,
      };
    }
  },
};

export function RuntimeProvider({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const runtime = useLocalRuntime(MyModelAdapter);

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      {children}
    </AssistantRuntimeProvider>
  );
}
