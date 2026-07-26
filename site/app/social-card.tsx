export const socialImageSize = {
  width: 1200,
  height: 630,
};

export const socialImageAlt =
  "Network Cartographer: see where every app connects on a live 3D globe";

export function SocialCard() {
  return (
    <div
      style={{
        alignItems: "stretch",
        background: "#0b0c0c",
        color: "#ebe8e0",
        display: "flex",
        fontFamily: "monospace",
        height: "100%",
        overflow: "hidden",
        padding: "72px 76px",
        position: "relative",
        width: "100%",
      }}
    >
      <div
        style={{
          background:
            "radial-gradient(circle at 74% 46%, rgba(224,168,106,0.16), transparent 34%)",
          display: "flex",
          inset: 0,
          position: "absolute",
        }}
      />

      <div
        style={{
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          position: "relative",
          width: "66%",
        }}
      >
        <div
          style={{
            color: "#e0a86a",
            display: "flex",
            fontSize: 24,
            letterSpacing: "0.08em",
            textTransform: "uppercase",
          }}
        >
          Network Cartographer
        </div>
        <div style={{ display: "flex", flexDirection: "column" }}>
          <div
            style={{
              display: "flex",
              fontSize: 58,
              fontWeight: 500,
              letterSpacing: "-0.045em",
              lineHeight: 1.05,
            }}
          >
            See where every app connects.
          </div>
          <div
            style={{
              color: "#9c9b96",
              display: "flex",
              fontSize: 25,
              lineHeight: 1.4,
              marginTop: 28,
            }}
          >
            Live network paths on a local 3D globe. One command. No account.
          </div>
        </div>
        <div style={{ color: "#74736f", display: "flex", fontSize: 21 }}>
          mapmy.network
        </div>
      </div>

      <div
        style={{
          alignItems: "center",
          display: "flex",
          justifyContent: "center",
          position: "relative",
          width: "34%",
        }}
      >
        <div
          style={{
            alignItems: "center",
            border: "2px solid rgba(235,232,224,0.52)",
            borderRadius: "50%",
            display: "flex",
            height: 300,
            justifyContent: "center",
            position: "relative",
            width: 300,
          }}
        >
          <div
            style={{
              border: "2px solid rgba(235,232,224,0.18)",
              borderRadius: "50%",
              display: "flex",
              height: 298,
              position: "absolute",
              width: 120,
            }}
          />
          <div
            style={{
              background: "rgba(235,232,224,0.16)",
              display: "flex",
              height: 2,
              position: "absolute",
              width: 296,
            }}
          />
          <div
            style={{
              border: "5px solid #e0a86a",
              borderBottomColor: "transparent",
              borderLeftColor: "transparent",
              borderRadius: "50%",
              display: "flex",
              height: 220,
              position: "absolute",
              transform: "rotate(-18deg)",
              width: 250,
            }}
          />
          <div
            style={{
              background: "#e0a86a",
              borderRadius: "50%",
              boxShadow: "0 0 28px rgba(224,168,106,0.55)",
              display: "flex",
              height: 16,
              position: "absolute",
              right: 20,
              top: 94,
              width: 16,
            }}
          />
        </div>
      </div>
    </div>
  );
}
