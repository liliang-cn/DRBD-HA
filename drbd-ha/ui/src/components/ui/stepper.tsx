import { Check } from 'lucide-react';
import * as React from 'react';

import { cn } from '@/lib/utils';

export interface StepperStep {
  title: string;
  description?: string;
}

export interface StepperProps extends React.HTMLAttributes<HTMLDivElement> {
  steps: StepperStep[];
  current: number;
}

const Stepper = React.forwardRef<HTMLDivElement, StepperProps>(
  ({ steps, current, className, ...props }, ref) => (
    <div
      ref={ref}
      className={cn('flex w-full items-start', className)}
      {...props}
    >
      {steps.map((step, index) => {
        const isCompleted = index < current;
        const isActive = index === current;
        const isLast = index === steps.length - 1;

        return (
          <React.Fragment key={`${step.title}-${index}`}>
            <div className="flex flex-col items-center text-center">
              <div
                className={cn(
                  'flex h-8 w-8 shrink-0 items-center justify-center rounded-full border-2 text-sm font-medium transition-colors',
                  isCompleted &&
                    'border-primary bg-primary text-primary-foreground',
                  isActive && 'border-primary text-primary',
                  !isCompleted &&
                    !isActive &&
                    'border-muted-foreground/30 text-muted-foreground',
                )}
              >
                {isCompleted ? (
                  <Check className="h-4 w-4" />
                ) : (
                  <span>{index + 1}</span>
                )}
              </div>
              <div className="mt-2 max-w-[8rem]">
                <div
                  className={cn(
                    'text-sm font-medium',
                    isActive || isCompleted
                      ? 'text-foreground'
                      : 'text-muted-foreground',
                  )}
                >
                  {step.title}
                </div>
                {step.description && (
                  <div className="mt-0.5 text-xs text-muted-foreground">
                    {step.description}
                  </div>
                )}
              </div>
            </div>
            {!isLast && (
              <div
                className={cn(
                  'mt-4 h-0.5 flex-1 transition-colors',
                  index < current ? 'bg-primary' : 'bg-muted-foreground/30',
                )}
              />
            )}
          </React.Fragment>
        );
      })}
    </div>
  ),
);
Stepper.displayName = 'Stepper';

export { Stepper };
