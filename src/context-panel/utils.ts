import { useReactFlow } from "@xyflow/react";
import { useNodeDataOverride } from "../NodeDataContext";
import type { ContractImage } from "../types";

/** Use context override (code-level rack) or fall back to ReactFlow's updateNodeData */
export function useUpdateNodeData(): (id: string, data: Record<string, unknown>) => void {
  const override = useNodeDataOverride();
  const rf = useReactFlow();
  return override ?? rf.updateNodeData;
}

/** Sanitize to camelCase / snake_case: only [a-zA-Z0-9_], first char must be lowercase letter */
export function sanitizeIdentifier(raw: string): string {
  const stripped = raw.replace(/[^a-zA-Z0-9_]/g, "");
  if (stripped.length === 0) return "";
  const first = stripped[0];
  if (/[a-zA-Z]/.test(first)) return first.toLowerCase() + stripped.slice(1);
  return stripped.slice(1);
}

/** Like sanitizeIdentifier but allows PascalCase (uppercase first letter) */
export function sanitizeTypeName(raw: string): string {
  const stripped = raw.replace(/[^a-zA-Z0-9_]/g, "");
  if (stripped.length === 0) return "";
  const first = stripped[0];
  if (/[a-zA-Z]/.test(first)) return first + stripped.slice(1);
  return stripped.slice(1);
}

const MAX_IMAGE_DIM = 1280;

export function resizeImage(file: File): Promise<ContractImage> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      let { width, height } = img;
      if (width <= MAX_IMAGE_DIM && height <= MAX_IMAGE_DIM) {
        const reader = new FileReader();
        reader.onload = () => {
          const b64 = (reader.result as string).split(",")[1];
          if (b64) resolve({ filename: file.name, mimeType: file.type || "image/png", data: b64 });
        };
        reader.readAsDataURL(file);
        return;
      }
      const scale = Math.min(MAX_IMAGE_DIM / width, MAX_IMAGE_DIM / height);
      width = Math.round(width * scale);
      height = Math.round(height * scale);
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d")!;
      ctx.drawImage(img, 0, 0, width, height);
      const dataUrl = canvas.toDataURL("image/jpeg", 0.85);
      resolve({ filename: file.name, mimeType: "image/jpeg", data: dataUrl.split(",")[1]! });
    };
    img.src = URL.createObjectURL(file);
  });
}
