const STORAGE_KEY = 'cerp_positions_v2'

export interface StoredPosition {
  commitment: string  // 64-char hex — the on-chain position key
  wallet: string      // Stellar public key that opened it
  symbol: string
  side: 0 | 1
  leverage: number
  openedAt: number
}

function load(): StoredPosition[] {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]')
  } catch {
    return []
  }
}

function save(positions: StoredPosition[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(positions))
}

export const positionsStore = {
  all(): StoredPosition[] {
    return load()
  },

  forWallet(publicKey: string): StoredPosition[] {
    return load().filter((p) => p.wallet === publicKey)
  },

  add(p: StoredPosition) {
    const existing = load()
    if (!existing.find((x) => x.commitment === p.commitment)) {
      save([...existing, p])
    }
  },

  remove(commitment: string) {
    save(load().filter((p) => p.commitment !== commitment))
  },
}
