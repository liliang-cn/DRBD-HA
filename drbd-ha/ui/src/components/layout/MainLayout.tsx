import { ApiOutlined, MoonOutlined, SunOutlined } from "@ant-design/icons";
import {
  theme as antdTheme,
  Button,
  ConfigProvider,
  Layout,
  Tooltip,
} from "antd";
import gsap from "gsap";
import { useEffect, useRef } from "react";
import { Link, Outlet } from "react-router-dom";
import { useThemeStore } from "@/stores/theme";
import { PRIMARY_COLOR } from "@/theme/colors";

const { Header, Content } = Layout;

export function MainLayout() {
  const headerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const { theme: currentTheme, toggleTheme } = useThemeStore();

  useEffect(() => {
    // Apply theme to document
    if (currentTheme === "dark") {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  }, [currentTheme]);

  useEffect(() => {
    // Animate header on mount
    if (headerRef.current) {
      gsap.fromTo(
        headerRef.current,
        { y: -100, opacity: 0 },
        { y: 0, opacity: 1, duration: 0.6, ease: "power3.out" }
      );
    }

    // Animate content on mount
    if (contentRef.current) {
      gsap.fromTo(
        contentRef.current,
        { y: 30, opacity: 0 },
        { y: 0, opacity: 1, duration: 0.5, ease: "power2.out", delay: 0.2 }
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
    <ConfigProvider
      theme={{
        algorithm:
          currentTheme === "dark"
            ? antdTheme.darkAlgorithm
            : antdTheme.defaultAlgorithm,
        token: {
          colorPrimary: PRIMARY_COLOR,
          colorSuccess: "#5FD4A9",
          colorWarning: "#E1C047",
          colorError: "#FF6D6D",
          borderRadius: 8,
        },
      }}
    >
      <Layout
        className={`min-h-screen ${
          currentTheme === "dark" ? "bg-slate-900" : "bg-slate-50"
        }`}
      >
        <Layout className="relative z-10">
          {/* Header */}
          <Header
            ref={headerRef}
            style={{
              backgroundColor: currentTheme === "dark" ? "#1e293b" : "#fff",
              borderBottom:
                currentTheme === "dark"
                  ? "1px solid #334155"
                  : "1px solid #e2e8f0",
            }}
            className="!px-0 !h-auto !py-0 sticky top-0 z-50 shadow-sm"
          >
            <div className="flex items-center justify-between px-4 py-2">
              {/* Logo & Brand */}
              <Link
                to="/"
                className="flex items-center gap-2"
                style={{ textDecoration: "none" }}
              >
                <img src="/favicon.svg" alt="DRBD HA" className="w-7 h-7" />
                <span
                  className={`text-base font-semibold ${
                    currentTheme === "dark" ? "text-white" : "text-slate-800"
                  }`}
                >
                  DRBD HA Manager
                </span>
              </Link>

              {/* Actions */}
              <div className="flex items-center gap-1">
                <Tooltip title="API Documentation">
                  <Button
                    type="text"
                    icon={<ApiOutlined />}
                    onClick={() => window.open("/swagger-ui/", "_blank")}
                  />
                </Tooltip>
                <Tooltip
                  title={currentTheme === "dark" ? "Light Mode" : "Dark Mode"}
                >
                  <Button
                    type="text"
                    icon={
                      currentTheme === "dark" ? (
                        <SunOutlined />
                      ) : (
                        <MoonOutlined />
                      )
                    }
                    onClick={toggleTheme}
                  />
                </Tooltip>
              </div>
            </div>
          </Header>

          {/* Content */}
          <Content
            className="p-4"
            ref={contentRef}
            style={{
              backgroundColor: currentTheme === "dark" ? "#0f172a" : "#f8fafc",
            }}
          >
            <Outlet />
          </Content>
        </Layout>
      </Layout>
    </ConfigProvider>
  );
}
