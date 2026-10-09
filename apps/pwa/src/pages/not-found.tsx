export function NotFound() {
  return (
    <main class="mx-auto flex min-h-dvh max-w-xl flex-col justify-center gap-4 p-6">
      <h1 class="font-display text-3xl font-extrabold text-navy-900">No encontramos esta página</h1>
      <a href="/" class="min-h-touch text-lg font-bold text-orange-700 underline">
        Ir al inicio para pedir ayuda
      </a>
    </main>
  );
}
