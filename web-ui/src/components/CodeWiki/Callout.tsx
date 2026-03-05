import { InformationCircleIcon, ExclamationTriangleIcon, CheckCircleIcon, XCircleIcon } from '@heroicons/react/24/outline';

type CalloutType = 'info' | 'warning' | 'success' | 'error';

interface CalloutProps {
  type: CalloutType;
  title?: string;
  children: React.ReactNode;
}

const calloutStyles = {
  info: {
    container: 'bg-blue-50 border-blue-200',
    icon: 'text-blue-600',
    title: 'text-blue-900',
    text: 'text-blue-800',
    Icon: InformationCircleIcon,
  },
  warning: {
    container: 'bg-amber-50 border-amber-200',
    icon: 'text-amber-600',
    title: 'text-amber-900',
    text: 'text-amber-800',
    Icon: ExclamationTriangleIcon,
  },
  success: {
    container: 'bg-green-50 border-green-200',
    icon: 'text-green-600',
    title: 'text-green-900',
    text: 'text-green-800',
    Icon: CheckCircleIcon,
  },
  error: {
    container: 'bg-red-50 border-red-200',
    icon: 'text-red-600',
    title: 'text-red-900',
    text: 'text-red-800',
    Icon: XCircleIcon,
  },
};

export function Callout({ type, title, children }: CalloutProps) {
  const styles = calloutStyles[type];
  const Icon = styles.Icon;

  return (
    <div className={`rounded-lg border p-4 my-4 ${styles.container}`}>
      <div className="flex gap-3">
        <Icon className={`w-5 h-5 flex-shrink-0 ${styles.icon}`} />
        <div className="flex-1">
          {title && (
            <div className={`font-semibold mb-1 ${styles.title}`}>{title}</div>
          )}
          <div className={`text-sm ${styles.text}`}>{children}</div>
        </div>
      </div>
    </div>
  );
}