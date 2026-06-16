import { Inbox } from 'lucide-react';
import * as React from 'react';

import { cn } from '@/lib/utils';

export interface EmptyProps extends React.HTMLAttributes<HTMLDivElement> {
  description?: React.ReactNode;
  icon?: React.ReactNode;
}

const Empty = React.forwardRef<HTMLDivElement, EmptyProps>(
  ({ description = 'No data', icon, className, children, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        'flex flex-col items-center justify-center gap-3 px-4 py-10 text-center',
        className,
      )}
      {...props}
    >
      <div className="text-muted-foreground">
        {icon ?? <Inbox className="h-10 w-10" strokeWidth={1.5} />}
      </div>
      {description && (
        <div className="text-sm text-muted-foreground">{description}</div>
      )}
      {children && <div className="mt-1">{children}</div>}
    </div>
  ),
);
Empty.displayName = 'Empty';

export { Empty };
