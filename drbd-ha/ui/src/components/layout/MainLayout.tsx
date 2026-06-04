import gsap from 'gsap';
import { Moon, Plug, Sun } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { Link, Outlet } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useThemeStore } from '@/stores/theme';

export function MainLayout() {
  const headerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const { theme, toggleTheme } = useThemeStore();

  useEffect(() => {
    // Apply theme to document
    if (theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }, [theme]);

  useEffect(() => {
    // Animate header on mount
    if (headerRef.current) {
      gsap.fromTo(
        headerRef.current,
        { y: -100, opacity: 0 },
        { y: 0, opacity: 1, duration: 0.6, ease: 'power3.out' },
      );
    }

    // Animate content on mount
    if (contentRef.current) {
      gsap.fromTo(
        contentRef.current,
        { y: 30, opacity: 0 },
        { y: 0, opacity: 1, duration: 0.5, ease: 'power2.out', delay: 0.2 },
      );
    }
  }, []);

  // Disabled route change animation to avoid conflicts with page animations
  // useEffect(() => {
  //   if (contentRef.current) {
  //     gsap.fromTo(
  //       contentRef.current,
  //       { opacity: 0, scale: 0.98 },
  //       { opacity: 1, scale: 1, duration: 0.4, ease: 'power2.out' },
  //     );
  //   }
  // }, [location.pathname]);

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <header
        ref={headerRef}
        className="sticky top-0 z-50 bg-card border-b border-border shadow-sm"
      >
        <div className="flex items-center justify-between px-4 py-2">
          {/* Logo & Brand */}
          <Link to="/" className="flex items-center gap-2 no-underline">
            <img src="/favicon.svg" alt="DRBD HA" className="w-7 h-7" />
            <span className="text-base font-semibold text-foreground">
              DRBD HA Manager
            </span>
          </Link>

          {/* Actions */}
          <div className="flex items-center gap-1">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => window.open('/swagger-ui/', '_blank')}
                >
                  <Plug />
                </Button>
              </TooltipTrigger>
              <TooltipContent>API Documentation</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" onClick={toggleTheme}>
                  {theme === 'dark' ? <Sun /> : <Moon />}
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                {theme === 'dark' ? 'Light Mode' : 'Dark Mode'}
              </TooltipContent>
            </Tooltip>
          </div>
        </div>
      </header>

      {/* Content */}
      <div className="p-4" ref={contentRef}>
        <Outlet />
      </div>
    </div>
  );
}
