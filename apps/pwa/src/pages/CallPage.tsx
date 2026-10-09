import { MessageSquareText, Phone, RefreshCw, Video } from 'lucide-preact';
import { useEffect, useRef } from 'preact/hooks';
import { useLocation } from 'preact-iso';
import { Composer, MessageList } from '../components/Chat';
import { IncidentStatus } from '../components/IncidentStatus';
import { LocationNotice } from '../components/LocationNotice';
import { Button, PageTitle } from '../components/ui';
import { locale, t } from '../i18n';
import { requestCall, requestHelp } from '../services';
import { call } from '../state/conversation';
import { videoCallProvider } from '../video/provider';

/**
 * Videollamada LSM o llamada de voz. La PWA solo solicita la llamada y entrega
 * los datos de la sala al proveedor del otro equipo (ver src/video/provider.ts).
 * El chat sigue disponible debajo como respaldo.
 */
export function CallPage() {
  const { query } = useLocation();
  const media = query.m === 'audio' ? 'audio' : 'video';
  const container = useRef<HTMLDivElement>(null);
  const current = call.value;

  useEffect(() => {
    requestHelp(media === 'audio' ? 'audio' : 'lsm_video');
    requestCall(media);
  }, [media]);

  useEffect(() => {
    if (current?.status !== 'ready' || !current.join || !container.current) return;
    let handle: { close(): void } | null = null;
    container.current.dataset.label = t('video.open');
    videoCallProvider()
      .open({
        join: current.join,
        media,
        container: container.current,
        language: locale.value,
        onEnded: () => {
          if (call.value) call.value = { ...call.value, status: 'ended' };
        },
      })
      .then((h) => {
        handle = h;
      })
      .catch(() => {
        if (call.value) call.value = { ...call.value, status: 'failed' };
      });
    return () => handle?.close();
  }, [current?.status, current?.join, media]);

  const title = media === 'audio' ? t('video.title_audio') : t('video.title_video');
  const status = current?.status ?? 'pending';
  const waiting = status !== 'ready' && status !== 'failed' && status !== 'ended';
  const tone =
    status === 'failed'
      ? 'border-danger bg-danger-soft'
      : status === 'ready'
        ? 'border-success bg-success-soft'
        : 'border-line-soft bg-surface';
  return (
    <>
      <PageTitle documentTitle={`${title} · ${t('app.name')}`}>{title}</PageTitle>
      <IncidentStatus />
      <div
        role="status"
        class={`sb-rise flex flex-col items-center gap-4 rounded-card border-2 px-4 py-6 text-center shadow-card ${tone}`}
      >
        <span
          aria-hidden="true"
          class={`relative grid h-20 w-20 place-items-center rounded-full ${media === 'audio' ? 'sb-tint-3' : 'sb-tint-1'} ${waiting ? 'sb-ring' : ''}`}
        >
          {media === 'audio' ? <Phone size={34} /> : <Video size={34} />}
        </span>
        <p class="text-xl font-extrabold">
          {status === 'ready'
            ? t('video.ready')
            : status === 'failed'
              ? t('video.failed')
              : status === 'ended'
                ? t('video.ended')
                : t('video.requesting')}
        </p>
        <div ref={container} />
        {status === 'failed' || status === 'ended' ? (
          <Button variant="accent" onClick={() => requestCall(media)}>
            <RefreshCw size={20} aria-hidden="true" />
            {title}
          </Button>
        ) : null}
      </div>
      <LocationNotice />
      <h2 class="flex items-center gap-2 font-display text-lg font-extrabold">
        <MessageSquareText size={20} aria-hidden="true" />
        {t('video.chat_below')}
      </h2>
      <MessageList />
      <Composer />
    </>
  );
}
