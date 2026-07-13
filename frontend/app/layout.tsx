import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Taskflow",
  description: "A simple task management app that helps you focus on what matters today.",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
