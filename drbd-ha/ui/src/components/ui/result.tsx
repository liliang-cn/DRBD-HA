import { AlertTriangle, CheckCircle2, Info, XCircle } from 'lucide-react';
import * as React from 'react';

import { cn } from '@/lib/utils';

type ResultStatus = 'success' | 'error' | 'info' | 'warning';

const statusConfig: Record<
  ResultStatus,
  { icon: React.ComponentType<{ className?: string }>; className: string }
> = {
  success: { icon: CheckCircle2, className: 'text-green-500' },
  error: { icon: XCircle, className: 'text-destructive' },
  info: { icon: Info, className: 'text-blue-500' },
  warning: { icon: AlertTriangle, className: 'text-yellow-500' },
};

export interface ResultProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, 'title'> {
  status?: ResultStatus;
  title?: React.ReactNode;
  subTitle?: React.ReactNode;
  extra?: React.ReactNode;
  icon?: React.ReactNode;
}

const Result = React.forwardRef<HTMLDivElement, ResultProps>(
  (
    {
      status = 'info',
      title,
      subTitle,
      extra,
      icon,
      className,
      children,
      ...props
    },
    ref,
  ) => {
    const config = statusConfig[status];
    const Icon = config.icon;

    return (
      <div
        ref={ref}
        className={cn(
          'flex flex-col items-center justify-center gap-4 px-6 py-12 text-center',
          className,
        )}
        {...props}
      >
        <div>
          {icon ?? <Icon className={cn('h-16 w-16', config.className)} />}
        </div>
        {title && (
          <div className="text-xl font-semibold text-foreground">{title}</div>
        )}
        {subTitle && (
          <div className="text-sm text-muted-foreground">{subTitle}</div>
        )}
        {children}
        {extra && (
          <div className="mt-2 flex items-center justify-center gap-3">
            {extra}
          </div>
        )}
      </div>
    );
  },
);
Result.displayName = 'Result';

export { Result };
