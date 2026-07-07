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

export const meta: Route.MetaFunction = () => [
  { title: 'Cerida — On-Chain Perpetuals' },
  {
    name: 'description',
    content:
      'Trade perpetual futures on crypto and real-world assets. Non-custodial, ZK-verified, TEE-matched, with optional shielded-pool privacy.',
  },
  { name: 'robots', content: 'index, follow' },
  { name: 'theme-color', content: '#04040d' },
  { property: 'og:site_name', content: 'Cerida' },
  { property: 'og:title', content: 'Cerida — On-Chain Perpetuals' },
  {
    property: 'og:description',
    content:
      'Trade perpetual futures on crypto and real-world assets. Non-custodial, ZK-verified, TEE-matched, with optional shielded-pool privacy.',
  },
  { property: 'og:image', content: 'https://ceridapp.xyz/prev_x.png' },
  { property: 'og:url', content: 'https://ceridapp.xyz' },
  { property: 'og:type', content: 'website' },
  { name: 'twitter:card', content: 'summary_large_image' },
  { name: 'twitter:title', content: 'Cerida — On-Chain Perpetuals' },
  {
    name: 'twitter:description',
    content:
      'Trade perpetual futures on crypto and real-world assets. Non-custodial, ZK-verified, TEE-matched.',
  },
  { name: 'twitter:image', content: 'https://ceridapp.xyz/prev_x.png' },
];

export const Layout = ({ children }: { children: ReactNode }) => (
  <html lang="en" className="h-full antialiased">
    <head>
      <meta charSet="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <link
        rel="icon"
        href="/favicon.png?v=3"
        type="image/png"
        sizes="512x512"
      />
      <link
        rel="shortcut icon"
        href="/favicon.png?v=3"
        type="image/png"
        sizes="512x512"
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
