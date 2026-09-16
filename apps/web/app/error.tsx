'use client';
export default function ErrorPage({ reset }: { reset: () => void }) {
  return (
    <>
      <h1>No podemos consultar la información.</h1>
      <p>
        La fuente puede estar temporalmente fuera de servicio. Inténtalo de nuevo en unos minutos.
      </p>
      <button className="button" onClick={reset}>
        Reintentar
      </button>
    </>
  );
}
