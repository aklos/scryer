import { memo } from "react";
import { C4Node } from "./C4Node";
import { GroupNode } from "./GroupNode";

export const nodeTypes = {
  c4: memo(C4Node),
  groupBox: memo(GroupNode),
};
