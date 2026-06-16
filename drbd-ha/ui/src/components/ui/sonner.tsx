import { Toaster as Sonner, type ToasterProps } from 'sonner';

import { useThemeStore } from '@/stores/theme';

const Toaster = ({ ...props }: ToasterProps) => {
  const theme = useThemeStore((state) => state.theme);

  return (
    <Sonner
      theme={theme as ToasterProps['theme']}
      className="toaster group"
      richColors
      {...props}
    />
  );
};

export { Toaster };
