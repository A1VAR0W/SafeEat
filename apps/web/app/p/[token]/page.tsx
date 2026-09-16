import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { statusLabels, productDisclaimer } from '@safeeat/contracts';
import { getShare, publicUrl } from '../../../lib/share';
export const dynamic = 'force-dynamic';
type Props = { params: Promise<{ token: string }> };
export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { token } = await params;
  const data = await getShare(token);
  if (data.type !== 'product') notFound();
  const p = data.item;
  const title = `${p.name} · ${statusLabels[p.assessment.status].label}`;
  return {
    title,
    description: p.assessment.reasons.map((r) => r.message).join(' '),
    openGraph: {
      title,
      description: p.assessment.reasons[0]?.message,
      url: `${publicUrl()}/p/${token}`,
      type: 'website',
      images: p.imageUrl ? [{ url: p.imageUrl }] : [],
    },
    twitter: { card: 'summary_large_image' },
    robots: { index: false, follow: false },
    alternates: { canonical: `${publicUrl()}/p/${token}` },
  };
}
export default async function Page({ params }: Props) {
  const { token } = await params;
  const data = await getShare(token);
  if (data.type !== 'product') notFound();
  const p = data.item;
  const s = statusLabels[p.assessment.status];
  return (
    <>
      {p.imageUrl && (
        <img className="product-image" src={p.imageUrl} alt={p.name} width={220} height={220} />
      )}
      <p className="eyebrow">Producto compartido</p>
      <h1>{p.name}</h1>
      <p className="muted">{p.brand}</p>
      {(p.stale || Date.now() - new Date(p.fetchedAt).getTime() > 86400000) && (
        <p className="notice">
          Información guardada que puede estar desactualizada. Comprueba la etiqueta actual.
        </p>
      )}
      <section className={`card assessment ${p.assessment.status}`}>
        <h2 className="status">
          {s.icon} {s.label}
        </h2>
        <p className="muted">
          Confianza en la evidencia: {Math.round(p.assessment.confidence * 100)} %
        </p>
        {p.assessment.reasons.map((r) => (
          <p key={r.code}>{r.message}</p>
        ))}
        <h2>La evidencia</h2>
        {p.assessment.evidence.length ? (
          p.assessment.evidence.map((e, i) => (
            <div key={i} className="evidence">
              <p>“{e.text}”</p>
              <span className="muted">{e.source}</span>
            </div>
          ))
        ) : (
          <p>No hay evidencia suficiente.</p>
        )}
        <p className="muted">{productDisclaimer}</p>
      </section>
      <section className="card">
        <h2>Calidad nutricional</h2>
        <div className="score">
          {p.quality.score === null ? 'Sin datos suficientes' : `${p.quality.score}/100`}
        </div>
        <p className="muted">{p.quality.explanation}</p>
        {p.nutriScore && (
          <p>
            Nutri-Score del proveedor: <strong>{p.nutriScore}</strong>
          </p>
        )}
        {p.quality.breakdown.length > 0 && (
          <ul>
            {p.quality.breakdown.map((b) => (
              <li key={b.label}>
                {b.label}: {b.points > 0 ? '+' : ''}
                {b.points}
              </li>
            ))}
          </ul>
        )}
      </section>
      {p.ingredients && (
        <section className="card">
          <h2>Ingredientes</h2>
          <p>{p.ingredients}</p>
        </section>
      )}
      <a className="button" href={`safeeat://p/${token}`}>
        Abrir en SafeEat
      </a>
      <p className="muted">
        {p.sourceUrl ? (
          <a href={p.sourceUrl} rel="noreferrer">
            {p.source}
          </a>
        ) : (
          p.source
        )}{' '}
        · Consultado el {new Date(p.fetchedAt).toLocaleDateString('es')}
      </p>
    </>
  );
}
