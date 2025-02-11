"use client";

import { Button } from "@repo/design-system/components/ui/button";
import { useState } from "react";

async function* backendApi({
  messages,
  abortSignal,
  config,
}: {
  messages: any;
  abortSignal?: AbortSignal;
  config?: any;
}) {
  const response = await fetch("/api/report", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      message: messages.at(-1).content[0].text,
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
            node: parsed.node,
            choices: [
              {
                delta: { content: parsed.response, node: parsed.node },
              },
              // {
              //   delta: { content: parsed.structured_response },
              // },
              // {
              //   delta: { content: JSON.stringify(parsed.tool_call) },
              // },
            ],
          };
        }
      }
    }
  }
}

const Client = () => {
  const [responseText, setResponseText] = useState("");

  const handleClick = async () => {
    setResponseText("");
    const abortController = new AbortController();
    const messages = [{ content: [{ text: "Generate a report" }] }];

    let lastNodeId = null;

    try {
      for await (const chunk of backendApi({
        messages,
        abortSignal: abortController.signal,
      })) {
        const newText = chunk.choices[0].delta?.content || "";
        const nodeId = chunk.node || chunk.choices[0].delta?.node || null;

        setResponseText((prev) => {
          const needsBreak = lastNodeId && lastNodeId !== nodeId ? " \n\n" : "";
          lastNodeId = nodeId;
          return prev + needsBreak + newText;
        });
      }
    } catch (error) {
      console.error("Streaming error:", error);
    }
  };

  return (
    <div>
      <Button onClick={handleClick}>Generate report</Button>
      <pre className="p-4 mt-2 border rounded text-sm whitespace-break-spaces">
        {responseText || "Waiting for response..."}
      </pre>
    </div>
  );
};

export default Client;
