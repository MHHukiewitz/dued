export function getUser(id: number): string {
  return fetch("/users/" + id).toString();
}

export function process(items: number[]): number {
  let total = 0;
  for (const item of items) {
    if (item > 0) {
      total += item;
    }
  }
  return total;
}

function unusedTs(): void {
  return;
}

export function main(): void {
  getUser(1);
  process([1, 2, 3]);
}
