'use client';

import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from '@repo/design-system/components/ui/avatar';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@repo/design-system/components/ui/tooltip';
import { tailwind } from '@repo/tailwind-config';

type PresenceAvatarProps = {
  info?: any
};

const PresenceAvatar = ({ info }: PresenceAvatarProps) => (
  <Tooltip delayDuration={0}>
    <TooltipTrigger>
      <Avatar className="h-7 w-7 bg-secondary ring-1 ring-background">
        <AvatarImage src={info?.avatar} alt={info?.name} />
        <AvatarFallback className="text-xs">
          {info?.name?.slice(0, 2)}
        </AvatarFallback>
      </Avatar>
    </TooltipTrigger>
    <TooltipContent collisionPadding={4}>
      <p>{info?.name ?? 'Unknown'}</p>
    </TooltipContent>
  </Tooltip>
);

export const AvatarStack = () => {
  return (
    <div className="-space-x-1 flex items-center px-4">
      {self && <PresenceAvatar info={self.info} />}
    </div>
  );
};
