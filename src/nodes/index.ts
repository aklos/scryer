import { memo } from "react";
import { C4Node } from "./C4Node";
import { GroupNode } from "./GroupNode";
import { ProcessNode } from "./ProcessNode";
import { ModelNode } from "./ModelNode";
import { OperationNode } from "./OperationNode";

export const nodeTypes = {
  c4: memo(C4Node),
  groupBox: memo(GroupNode),
  process: memo(ProcessNode),
  model: memo(ModelNode),
  operation: memo(OperationNode),
};
