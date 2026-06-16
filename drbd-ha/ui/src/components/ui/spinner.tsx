import { Loader2 } from 'lucide-react';
import * as React from 'react';

import { cn } from '@/lib/utils';

export interface SpinnerProps extends React.HTMLAttributes<SVGSVGElement> {
  size?: number;
}

const Spinner = React.forwardRef<SVGSVGElement, SpinnerProps>(
  ({ className, size = 16, ...props }, ref) => (
    <Loader2
      ref={ref}
      width={size}
      height={size}
      className={cn('animate-spin text-muted-foreground', className)}
      {...props}
    />
  ),
);
Spinner.displayName = 'Spinner';

export { Spinner };
