import { Phone, Video } from 'lucide-preact';
import { useEffect, useRef } from 'preact/hooks';
import { useLocation } from 'preact-iso';
import { Composer, MessageList } from '../components/Chat';
import { IncidentStatus } from '../components/IncidentStatus';
import { LocationNotice } from '../components/LocationNotice';
import { Button, Notice, PageTitle } from '../components/ui';
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
  return (
    <>
      <PageTitle documentTitle={`${title} · ${t('app.name')}`}>{title}</PageTitle>
      <IncidentStatus />
      <Notice
        tone={status === 'failed' ? 'warning' : status === 'ready' ? 'success' : 'info'}
        icon={media === 'audio' ? <Phone size={22} /> : <Video size={22} />}
        live
      >
        <p class="text-lg font-bold">
          {status === 'ready'
            ? t('video.ready')
            : status === 'failed'
              ? t('video.failed')
              : status === 'ended'
                ? t('video.ended')
                : t('video.requesting')}
        </p>
        <div ref={container} class="mt-3" />
        {status === 'failed' || status === 'ended' ? (
          <Button variant="accent" class="mt-3" onClick={() => requestCall(media)}>
            {title}
          </Button>
        ) : null}
      </Notice>
      <LocationNotice />
      <p class="font-bold">{t('video.chat_below')}</p>
      <MessageList />
      <Composer />
    </>
  );
}
