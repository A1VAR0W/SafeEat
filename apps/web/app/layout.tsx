import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import './style.css';
export const metadata: Metadata = {
  title: 'SafeEat · La información que necesitas',
  description:
    'Consulta ingredientes, evidencias sobre gluten y calidad nutricional de forma independiente.',
};
export default function Layout({ children }: { children: ReactNode }) {
  return (
    <html lang="es">
      <body>
        <header>
          <a className="brand" href="/">
            ◒ SafeEat
          </a>
          <span>Conoce lo que comes.</span>
        </header>
        <main>{children}</main>
        <footer>
          SafeEat · Comprueba siempre la etiqueta actual.
          <br />
          <a href="/privacy">Privacidad</a> · <a href="/sources">Fuentes y metodología</a>
        </footer>
      </body>
    </html>
  );
}
