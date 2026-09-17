import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import Image from 'next/image';
import './style.css';
export const metadata: Metadata = {
  title: 'SafeEat · La información que necesitas',
  description:
    'Consulta ingredientes, evidencias sobre gluten y calidad nutricional de forma independiente.',
};
export default function Layout({ children }: { children: ReactNode }) {
  const base = process.env.WEB_BASE_PATH || '';
  return (
    <html lang="es">
      <body>
        <header>
          <a className="brand" href={`${base}/`}>
            <Image src={`${base}/brand/safeeat-mark.png`} alt="" width={34} height={34} priority />
            <Image
              src={`${base}/brand/safeeat-wordmark.png`}
              alt="SafeEat"
              width={118}
              height={26}
              priority
            />
          </a>
          <span>Conoce lo que comes.</span>
        </header>
        <main>{children}</main>
        <footer>
          SafeEat · Comprueba siempre la etiqueta actual.
          <br />
          <a href={`${base}/privacy`}>Privacidad</a> ·{' '}
          <a href={`${base}/sources`}>Fuentes y metodología</a>
        </footer>
      </body>
    </html>
  );
}
