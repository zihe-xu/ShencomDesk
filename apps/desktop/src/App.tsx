import { useState } from "react";

import { Login } from "@/components/Login";
import { ImageCompression } from "@/components/image-compression/ImageCompression";
import { Toaster } from "@/components/ui/sonner";

function App() {
  const [displayName, setDisplayName] = useState<string | null>(null);

  return (
    <>
      {displayName ? (
        <ImageCompression displayName={displayName} />
      ) : (
        <Login onSuccess={setDisplayName} />
      )}
      <Toaster />
    </>
  );
}

export default App;
