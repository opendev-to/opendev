import React, { forwardRef } from 'react';

function cn(...classes: Array<string | false | null | undefined>) {
  return classes.filter(Boolean).join(' ');
}

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  leftIcon?: React.ReactNode;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { leftIcon, className, ...props },
  ref
) {
  return (
    <div className={cn('relative', className)}>
      {leftIcon && (
        <span className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none text-gray-400">
          {leftIcon}
        </span>
      )}
      <input
        ref={ref}
        {...props}
        className={cn(
          'w-full bg-white text-gray-900 rounded-lg border border-gray-200 focus:border-primary-500 focus:ring-2 focus:ring-primary-100 outline-none',
          leftIcon ? 'pl-9 pr-3 py-2 text-sm' : 'px-3 py-2 text-sm'
        )}
      />
    </div>
  );
});
