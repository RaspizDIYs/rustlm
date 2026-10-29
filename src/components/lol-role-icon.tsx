import { ROLES } from "@/lib/automation";

// Same Riot position geometry as GoodLuck's role-paths.ts.
const paths: Record<
  keyof typeof ROLES,
  { dim?: string; active: string; activeIsPolygon?: boolean }
> = {
  top: {
    dim: "M21,14H14v7h7V14Zm5-3V26L11.014,26l-4,4H30V7.016Z",
    active: "4 4 4.003 28.045 9 23 9 9 23 9 28.045 4.003 4 4",
    activeIsPolygon: true,
  },
  jungle: {
    active:
      "M25,3c-2.128,3.3-5.147,6.851-6.966,11.469A42.373,42.373,0,0,1,20,20a27.7,27.7,0,0,1,1-3C21,12.023,22.856,8.277,25,3ZM13,20c-1.488-4.487-4.76-6.966-9-9,3.868,3.136,4.422,7.52,5,12l3.743,3.312C14.215,27.917,16.527,30.451,17,31c4.555-9.445-3.366-20.8-8-28C11.67,9.573,13.717,13.342,13,20Zm8,5a15.271,15.271,0,0,1,0,2l4-4c0.578-4.48,1.132-8.864,5-12C24.712,13.537,22.134,18.854,21,25Z",
  },
  middle: {
    dim: "M30,12.968l-4.008,4L26,26H17l-4,4H30ZM16.979,8L21,4H4V20.977L8,17,8,8h8.981Z",
    active: "25 4 4 25 4 30 9 30 30 9 30 4 25 4",
    activeIsPolygon: true,
  },
  bottom: {
    dim: "M13,20h7V13H13v7ZM4,4V26.984l3.955-4L8,8,22.986,8l4-4H4Z",
    active: "29.997 5.955 25 11 25 25 11 25 5.955 29.997 30 30 29.997 5.955",
    activeIsPolygon: true,
  },
  utility: {
    active:
      "M26,13c3.535,0,8-4,8-4H23l-3,3,2,7,5-2-3-4h2ZM22,5L20.827,3H13.062L12,5l5,6Zm-5,9-1-1L13,28l4,3,4-3L18,13ZM11,9H0s4.465,4,8,4h2L7,17l5,2,2-7Z",
  },
};
export function LolRoleIcon({ role }: { role: keyof typeof ROLES }) {
  const path = paths[role];
  return (
    <svg viewBox="0 0 34 34" className="size-4 shrink-0" aria-hidden="true">
      {path.dim && (
        <path
          opacity="0.45"
          fill="currentColor"
          fillRule="evenodd"
          d={path.dim}
        />
      )}
      {path.activeIsPolygon ? (
        <polygon fill="currentColor" points={path.active} />
      ) : (
        <path fill="currentColor" fillRule="evenodd" d={path.active} />
      )}
    </svg>
  );
}
