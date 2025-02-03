"use client";

import {
  ComposerPrimitive,
  MessagePrimitive,
  ThreadPrimitive,
  ThreadAssistantMessage,
  useThread,
  UIContentPart,
} from "@assistant-ui/react";
import { useEffect, useState, type FC, type PropsWithChildren } from "react";

import { SendHorizonalIcon } from "lucide-react";
import { Button } from "@repo/design-system/components/ui/button";

export const Thread: FC = (props) => {
  useEffect(() => {
    window.addEventListener("action_plan_click", (event: Event) => {
      console.log(event);
      // setActionPlan((event as any).detail);
    });
  }, []);

  return (
    <ThreadPrimitive.Root className="bg-background h-full">
      <ThreadPrimitive.Viewport className="flex h-full flex-col items-center overflow-y-scroll scroll-smooth px-4 pt-8">
        <ThreadWelcome />
        <ThreadPrimitive.Messages
          components={{
            UserMessage,
            AssistantMessage,
          }}
        />
        <div className="sticky bottom-0 mt-4 flex w-full max-w-2xl flex-grow flex-col items-center justify-end rounded-t-lg bg-inherit pb-4">
          <ThreadPrimitive.If empty={false}>
            <div className="mb-4 w-full">
              <div className="flex flex-wrap justify-center gap-4">
                <ThreadPrimitive.If running={false}>
                  {" "}
                  {/*Important to wrap Thread suggestion into if statement since the original message is streamed and we don't want to generate buttons ahead of time*/}
                  <StructuredOutput />
                </ThreadPrimitive.If>
              </div>
            </div>
            <Composer />
          </ThreadPrimitive.If>
        </div>
      </ThreadPrimitive.Viewport>
    </ThreadPrimitive.Root>
  );
};

const ThreadSuggestion: FC<PropsWithChildren<{ prompt: string }>> = ({
  prompt,
  children,
}) => {
  return (
    <ThreadPrimitive.Suggestion
      prompt={prompt}
      method="replace"
      autoSend
      asChild
    >
      <Button variant="outline" className="flex-1 h-12">
        {children}
      </Button>
    </ThreadPrimitive.Suggestion>
  );
};

const ThreadWelcome: FC = () => {
  return (
    <div className="flex w-full max-w-3xl grow flex-col px-4 py-6">
      <ThreadPrimitive.Empty>
        <div className="flex flex-grow basis-full flex-col items-center justify-center">
          <h1 className="leading-tighter mb-4 text-center text-5xl font-extrabold tracking-tighter md:text-6xl">
            <span className="bg-gradient-to-r from-green-500 to-teal-400 bg-clip-text text-transparent">
              touchbase.chat
            </span>
          </h1>
          <h2 className="text-center text-lg">
            <div>
              A <i>proactive</i> AI assistant to help you manage your tasks and
              goals—big or small—so you can forget sticky notes, scattered apps,
              and mental overload.
            </div>
          </h2>
        </div>
        <div className="mb-4 w-full px-4">
          <div className="flex flex-wrap justify-center gap-4">
            <ThreadSuggestion prompt="Hello!">
              <p className="font-semibold">Get started</p>
            </ThreadSuggestion>
          </div>
        </div>
      </ThreadPrimitive.Empty>
    </div>
  );
};

const useLastAssistantMessage = () => {
  return useThread(({ messages }) => {
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i]?.role === "assistant") {
        return messages[i]! as ThreadAssistantMessage;
      }
    }
    return null;
  });
};

const StructuredOutput: FC = () => {
  const lastAssistantMessage = useLastAssistantMessage();
  const [data, setData] = useState<{ ui_elements: any[] } | null>(null);
  const uiContent: UIContentPart | undefined =
    lastAssistantMessage?.content.find((c) => c.type === "ui");

  useEffect(() => {
    if (uiContent) {
      try {
        const str = (uiContent.display as any).props.children;
        const data = JSON.parse(str);
        if (data.ui_elements) {
          setData(data);
        }

        if (data.action_plan && data.action_plan.length) {
          const event = new CustomEvent("action_plan_update", {
            detail: data.action_plan,
          });
          dispatchEvent(event);
        }
      } catch (err) {
        //pass
      }
    }
  }, [uiContent]);

  if (!data) {
    return null;
  }

  return (
    <div className="flex w-full space-x-2">
      {data.ui_elements.map((e, i) => {
        switch (e.component) {
          case "prompt_suggestion":
            return (
              <ThreadPrimitive.Suggestion
                key={i}
                prompt={e.value}
                method="replace"
                autoSend
                asChild
              >
                <Button
                  variant="outline"
                  className="h-auto flex-1 p-2"
                  style={{ whiteSpace: "normal", wordWrap: "break-word" }}
                >
                  {e.label}
                </Button>
              </ThreadPrimitive.Suggestion>
            );
          case "toast":
            return <div key={i}>{e.label}</div>;
          default:
            return null;
        }
      })}
    </div>
  );
};

const HumanQuestion: FC = () => {
  const lastAssistantMessage = useLastAssistantMessage();
  const [data, setData] = useState<{ question: string } | null>(null);
  const uiContent: UIContentPart | undefined =
    lastAssistantMessage?.content.find((c) => c.type === "ui");

  useEffect(() => {
    if (uiContent && !data) {
      try {
        const str = (uiContent.display as any).props.children;
        const data = JSON.parse(str);
        if (data.question) {
          setData(data);
        }
      } catch (err) {
        //pass
      }
    }
  }, [uiContent, data]);

  if (!data) {
    return null;
  }

  return (
    <div className="mt-4 rounded-3xl px-5 py-2.5 italic text-center">
      {data.question}
    </div>
  );
};

const Composer: FC = () => {
  return (
    <ComposerPrimitive.Root className="flex w-full items-end rounded-lg border p-0.5 transition-shadow focus-within:shadow-sm">
      <ComposerPrimitive.Input
        placeholder="Write a message..."
        className="h-12 max-h-40 flex-grow resize-none bg-transparent p-3.5 text-sm outline-none placeholder:text-foreground/50"
      />
      <ComposerPrimitive.Send className="m-2 flex h-8 w-8 items-center justify-center rounded-md bg-foreground font-bold text-2xl shadow transition-opacity disabled:opacity-10">
        <SendHorizonalIcon className="size-4 text-background" />
      </ComposerPrimitive.Send>
    </ComposerPrimitive.Root>
  );
};

const UserMessage: FC = () => {
  return (
    <MessagePrimitive.Root className="my-4 grid w-full max-w-2xl auto-rows-auto grid-cols-[minmax(72px,1fr)_auto] gap-y-2">
      <div className="bg-muted text-foreground col-start-2 row-start-1 max-w-xl break-words rounded-3xl px-5 py-2.5">
        <MessagePrimitive.Content />
      </div>
    </MessagePrimitive.Root>
  );
};

const AssistantMessage: FC = () => {
  return (
    <MessagePrimitive.Root className="relative my-4 grid w-full max-w-2xl grid-cols-[auto_1fr] grid-rows-[auto_1fr]">
      {/* <Avatar className="col-start-1 row-span-full row-start-1 mr-4">
        <AvatarFallback>A</AvatarFallback>
      </Avatar> */}

      <div className="text-foreground col-start-2 row-start-1 my-1.5 max-w-xl break-words leading-7">
        <MessagePrimitive.Content />
        <HumanQuestion />
      </div>
    </MessagePrimitive.Root>
  );
};
