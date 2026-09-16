import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { restaurantDisclaimer } from '@safeeat/contracts';
import { getShare, publicUrl } from '../../../lib/share';
export const dynamic = 'force-dynamic';
type Props = { params: Promise<{ token: string }> };
export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { token } = await params;
  const data = await getShare(token);
  if (data.type !== 'restaurant') notFound();
  return {
    title: `${data.item.name} · SafeEat`,
    description: `${data.item.address}. ${restaurantDisclaimer}`,
    openGraph: {
      title: data.item.name,
      description: restaurantDisclaimer,
      url: `${publicUrl()}/r/${token}`,
      type: 'website',
    },
    robots: { index: false, follow: false },
  };
}
export default async function Page({ params }: Props) {
  const { token } = await params;
  const data = await getShare(token);
  if (data.type !== 'restaurant') notFound();
  const r = data.item;
  return (
    <>
      <p className="eyebrow">Restaurante compartido</p>
      <h1>{r.name}</h1>
      <p className="lede">{r.address}</p>
      <section className="card assessment UNKNOWN">
        <h2>
          {r.glutenSupport === 'UNKNOWN'
            ? '? Información sin verificar'
            : r.glutenSupport === 'DEDICATED_GLUTEN_FREE'
              ? 'Dedicado a comida sin gluten'
              : 'Opciones sin gluten'}
        </h2>
        {r.evidence.map((e, i) => (
          <p key={i}>
            {e.text} <span className="muted">· {e.source}</span>
          </p>
        ))}
        <p>{restaurantDisclaimer}</p>
      </section>
      <a className="button" href={r.mapsUrl} rel="noreferrer">
        Cómo llegar
      </a>
      <a className="button" href={`safeeat://r/${token}`}>
        Abrir en SafeEat
      </a>
      {r.website && (
        <p>
          <a href={r.website} rel="noreferrer">
            Web del restaurante
          </a>
        </p>
      )}
      <p className="muted">Google Maps · {r.attributions.join(' · ')}</p>
    </>
  );
}
