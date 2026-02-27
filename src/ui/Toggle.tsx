/** Small sliding toggle switch for boolean values. */
export function Toggle({ value, onChange }: {
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={value}
      onClick={() => onChange(!value)}
      className={`h-4 w-7 rounded-full transition-colors cursor-pointer flex items-center ${
        value ? "bg-blue-500 justify-end" : "bg-zinc-300 dark:bg-zinc-600 justify-start"
      }`}
    >
      <div className="h-3 w-3 rounded-full bg-white shadow-sm mx-0.5" />
    </button>
  );
}
