/**
 * Textos en español (idioma por defecto y fuente de verdad).
 * Lenguaje simple y claro: frases cortas, en segunda persona, sin tecnicismos.
 */
export const es = {
  'app.name': 'Sin Barreras',
  'app.program': 'Atención Incluyente de Emergencias · C5 Zapopan',
  'app.skip': 'Saltar al contenido',
  'app.back': 'Regresar',
  'app.settings': 'Ajustes',
  'app.loading': 'Cargando…',

  'home.title': '¿Cómo deseas comunicarte?',
  'home.subtitle': 'Elige una opción. Te conectamos con el C5 de inmediato.',
  'channel.lsm_video.title': 'Videollamada en Lengua de Señas',
  'channel.lsm_video.desc': 'Habla con un intérprete de LSM.',
  'channel.text_chat.title': 'Chat de texto',
  'channel.text_chat.desc': 'Escribe y recibe mensajes.',
  'channel.audio.title': 'Llamada de voz',
  'channel.audio.desc': 'Habla con el C5 por audio.',
  'channel.translation.title': 'Otro idioma',
  'channel.translation.desc': 'Other language · Autre langue · 其他语言',
  'channel.silent.title': 'Modo silencioso',
  'channel.silent.desc': 'Sin sonido. Para cuando no puedes hablar.',

  'privacy.short':
    'Para atenderte, guardamos la conversación y tu ubicación como respaldo de la emergencia.',
  'privacy.link': 'Aviso de privacidad',
  'privacy.title': 'Aviso de privacidad',

  'fallback.title': '¿No puedes conectarte?',
  'fallback.call': 'Llamar al {phone}',
  'fallback.sms': 'Enviar SMS al {phone}',

  'connection.connecting': 'Conectando con el C5…',
  'connection.online': 'Conectado con el C5',
  'connection.reconnecting': 'Reconectando… Tus mensajes se enviarán al volver la conexión.',
  'connection.offline': 'Sin internet. Tus mensajes se enviarán cuando vuelva la conexión.',
  'connection.outdated': 'Hay una versión nueva. Recarga la página para continuar.',
  'connection.reload': 'Recargar',

  'incident.received': 'Recibimos tu solicitud. En un momento te atiende una persona del C5.',
  'incident.attending': 'Te estamos atendiendo.',
  'incident.unit_en_route': 'Una unidad va en camino.',
  'incident.unit_en_route_eta': 'Una unidad va en camino. Llega en unos {minutes} min.',
  'incident.unit_on_scene': 'La unidad llegó al lugar.',
  'incident.closed': 'El incidente fue cerrado. Si necesitas algo más, escríbenos.',
  'incident.requesting': 'Enviando tu solicitud de ayuda…',

  'chat.title': 'Chat con el C5',
  'chat.log': 'Conversación',
  'chat.empty': 'Escribe qué está pasando y dónde estás.',
  'chat.input': 'Tu mensaje',
  'chat.send': 'Enviar',
  'chat.you': 'Tú',
  'chat.operator': 'C5',
  'chat.system': 'Aviso',
  'chat.status.queued': 'En cola: se enviará al reconectar',
  'chat.status.sent': 'Enviado',
  'chat.status.delivered': 'Entregado',
  'chat.status.read': 'Leído',
  'chat.status.failed': 'No se pudo entregar. Llama al {phone}.',
  'chat.new_operator_message': 'Nuevo mensaje del C5: {text}',

  'silent.title': 'Modo silencioso',
  'silent.hint': 'Toca un botón. El C5 recibe tu mensaje sin sonido.',
  'silent.quick.in_danger': 'Estoy en peligro',
  'silent.quick.cannot_speak': 'No puedo hablar',
  'silent.quick.being_followed': 'Alguien me sigue',
  'silent.quick.injured': 'Estoy herido',
  'silent.exit': 'Salida rápida',
  'silent.exit_hint': 'Cambia a una pantalla neutral. Tu solicitud sigue activa.',

  'neutral.title': 'Clima en Zapopan',
  'neutral.today': 'Hoy',
  'neutral.weather': 'Parcialmente nublado',
  'neutral.more': 'Ver pronóstico',

  'video.title_video': 'Videollamada en LSM',
  'video.title_audio': 'Llamada de voz',
  'video.requesting': 'Preparando la llamada… En un momento se conecta.',
  'video.ready': 'La llamada está lista.',
  'video.open': 'Entrar a la llamada',
  'video.failed': 'No se pudo preparar la llamada. Puedes escribir en el chat.',
  'video.ended': 'La llamada terminó.',
  'video.chat_below': 'También puedes escribir:',

  'translation.title': 'Elige tu idioma',
  'translation.hint': 'Choose your language · Choisissez votre langue',

  'location.why': 'Comparte tu ubicación para que la ayuda llegue más rápido.',
  'location.share': 'Compartir mi ubicación',
  'location.shared': 'Tu ubicación se está compartiendo con el C5.',
  'location.denied': 'No tenemos tu ubicación. Escribe dónde estás.',
  'location.qr': 'Usamos la ubicación del punto donde escaneaste el código.',

  'settings.title': 'Ajustes',
  'settings.language': 'Idioma',
  'settings.font': 'Tamaño de letra',
  'settings.font.100': 'Normal',
  'settings.font.125': 'Grande',
  'settings.font.150': 'Más grande',
  'settings.font.200': 'Muy grande',
  'settings.contrast': 'Alto contraste',
  'settings.contrast_hint': 'Colores con más diferencia para ver mejor.',

  'error.invalid': 'No pudimos enviar ese mensaje. Revísalo e inténtalo de nuevo.',
  'notfound.title': 'No encontramos esta página',
  'notfound.home': 'Ir al inicio para pedir ayuda',
} as const;

export type MessageKey = keyof typeof es;
