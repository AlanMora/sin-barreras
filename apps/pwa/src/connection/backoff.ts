/**
 * Backoff exponencial con jitter completo: espera aleatoria entre 0 y
 * min(max, base·2^intento). Evita que miles de teléfonos reconecten al mismo
 * tiempo cuando el servicio se recupera.
 */
export class Backoff {
  private attempt = 0;

  constructor(
    private readonly baseMs = 500,
    private readonly maxMs = 15_000,
    private readonly random: () => number = Math.random,
  ) {}

  next(): number {
    const ceiling = Math.min(this.maxMs, this.baseMs * 2 ** Math.min(this.attempt, 16));
    this.attempt += 1;
    // Al menos 100 ms para no entrar en un ciclo apretado.
    return Math.max(100, Math.floor(this.random() * ceiling));
  }

  reset(): void {
    this.attempt = 0;
  }

  get attempts(): number {
    return this.attempt;
  }
}
