export type DialogValue = boolean | string | null;

export interface DialogChoice {
  value: string;
  label: string;
}

export type DialogRequest = {
  id: number;
  kind: "confirm" | "input" | "alert" | "choice";
  title: string;
  message: string;
  choices?: DialogChoice[];
  initialValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  resolve: (value: DialogValue) => void;
};
