import {
  isRouteErrorResponse,
  Links,
  Meta,
  Outlet,
  Scripts,
  ScrollRestoration,
} from 'react-router';
import type { ReactNode } from 'react';
import type { Route } from './+types/root';
import './app.css';

export const Layout = ({ children }: { children: ReactNode }) => (
  <html lang="en" className="h-full antialiased">
    <head>
      <meta charSet="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <link
        rel="icon"
        href="/android-chrome-512x512?v=2"
        type="image/png"
        sizes="180x180"
      />
      <link
        rel="apple-touch-icon"
        href="/apple-touch-icon.png?v=2"
        sizes="180x180"
      />
      <Meta />
      <Links />
    </head>
    <body className="min-h-full text-text-primary">
      {children}
      <ScrollRestoration />
      <Scripts />
    </body>
  </html>
);

export default function App() {
  return <Outlet />;
}

export const ErrorBoundary = ({ error }: Route.ErrorBoundaryProps) => {
  let message = 'Error';
  let details = 'The trading app hit an unexpected error.';

  if (isRouteErrorResponse(error)) {
    message = error.status === 404 ? '404' : 'Route error';
    details = error.statusText || details;
  } else if (error instanceof Error) {
    details = error.message;
  }

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-3 p-8 bg-page">
      <h1 className="text-3xl font-bold">{message}</h1>
      <p className="max-w-xl text-center text-text-tertiary">{details}</p>
    </main>
  );
};
