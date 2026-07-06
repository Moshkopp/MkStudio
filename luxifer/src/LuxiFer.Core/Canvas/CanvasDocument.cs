namespace LuxiFer.Core.Canvas;

/// <summary>
/// Der Canvas ist kein Bild, sondern ein strukturiertes Dokument
/// aus Layern, Objekten und Gruppen.
/// </summary>
public sealed class CanvasDocument
{
    public double WidthMm { get; set; } = 600;
    public double HeightMm { get; set; } = 400;
    public List<Layer> Layers { get; } = [];

    /// <summary>
    /// Oberstes getroffenes Objekt (spätere Layer/Objekte liegen oben).
    /// Unsichtbare und gesperrte Layer werden übersprungen.
    /// </summary>
    public CanvasObject? HitTest(double x, double y, double tolerance = 0)
    {
        for (var l = Layers.Count - 1; l >= 0; l--)
        {
            var layer = Layers[l];
            if (!layer.Visible || layer.Locked) continue;
            for (var i = layer.Objects.Count - 1; i >= 0; i--)
                if (layer.Objects[i].HitTest(x, y, tolerance))
                    return layer.Objects[i];
        }
        return null;
    }
}

/// <summary>Bearbeitungsmodus eines Layers.</summary>
public enum LayerMode
{
    Cut,
    Fill,
    Raster,
}

/// <summary>Gestaltungsregeln, die sich aus dem Layer-Modus ergeben.</summary>
public static class LayerModeRules
{
    /// <summary>
    /// Formen auf Fill-/Raster-Layern werden flächig dargestellt; Cut-Layer
    /// nur als Kontur. So ist der Bearbeitungsmodus visuell sofort erkennbar
    /// (ADR 0003, §5).
    /// </summary>
    public static bool IsFilled(this LayerMode mode) =>
        mode is LayerMode.Fill or LayerMode.Raster;
}

/// <summary>
/// Ein Layer bündelt Objekte mit gemeinsamen Laser-Parametern —
/// der Layer bestimmt, WIE gelasert wird (Modus, Speed, Power).
/// </summary>
public sealed class Layer
{
    /// <summary>Standard-Farbpalette; neue Layer erhalten reihum eine Farbe.</summary>
    public static readonly string[] SwatchColors =
    [
        "#EF4444", "#3B82F6", "#10B981", "#EAB308", "#D946EF", "#A855F7", "#84CC16",
        "#06B6D4", "#F97316", "#8B5CF6", "#EC4899", "#00FFFF", "#F59E0B", "#6B7280",
    ];

    public Guid Id { get; init; } = Guid.NewGuid();
    public required string Name { get; set; }
    public string ColorHex { get; set; } = SwatchColors[0];
    public bool Visible { get; set; } = true;
    public bool Locked { get; set; }

    public LayerMode Mode { get; set; } = LayerMode.Cut;
    public double SpeedMmS { get; set; } = 100;
    public double PowerPct { get; set; } = 20;
    public double MinPowerPct { get; set; } = 10;
    public int Passes { get; set; } = 1;
    public bool AirAssist { get; set; }
    /// <summary>Zeilenabstand für Fill-Layer in mm.</summary>
    public double LineStepMm { get; set; } = 0.1;
    /// <summary>Auflösung für Raster-Layer.</summary>
    public double Dpi { get; set; } = 254;

    public List<CanvasObject> Objects { get; } = [];

    public static Layer CreateNext(int index) => new()
    {
        Name = $"Layer {index + 1}",
        ColorHex = SwatchColors[index % SwatchColors.Length],
    };
}

public abstract class CanvasObject
{
    public Guid Id { get; init; } = Guid.NewGuid();
    public double X { get; set; }
    public double Y { get; set; }
    public double Rotation { get; set; }

    /// <summary>
    /// Eigene Farbe des Objekts (ADR 0005). Beim Erzeugen wird die Vorgabefarbe
    /// des Layers kopiert; danach ist die Farbe objekt-eigen und pro Auswahl
    /// änderbar. Der Layer bestimmt weiterhin die Laserparameter, nicht die Farbe.
    /// </summary>
    public string ColorHex { get; set; } = Layer.SwatchColors[0];

    /// <summary>Achsenparallele Bounding-Box in mm (ohne Rotation).</summary>
    public abstract (double X, double Y, double Width, double Height) Bounds { get; }

    /// <summary>
    /// Ob das Objekt eine füllbare Fläche umschließt. Nur geschlossene Formen
    /// (Rechteck, Ellipse, geschlossene Polyline) werden auf Fill-Layern
    /// flächig dargestellt; Linien und offene Polylines bleiben Kontur.
    /// </summary>
    public virtual bool IsFillable => false;

    public virtual bool HitTest(double x, double y, double tolerance = 0)
    {
        var (bx, by, bw, bh) = Bounds;
        // Bei gedrehtem Objekt den Testpunkt in den ungedrehten Objektraum
        // zurückrotieren (um den Mittelpunkt) und gegen die Box prüfen.
        if (Rotation != 0)
        {
            var cx = bx + bw / 2;
            var cy = by + bh / 2;
            (x, y) = Geometry.RotatePoint(x, y, cx, cy, -Rotation);
        }
        return x >= bx - tolerance && x <= bx + bw + tolerance
            && y >= by - tolerance && y <= by + bh + tolerance;
    }

    public virtual void MoveBy(double dx, double dy)
    {
        X += dx;
        Y += dy;
    }

    /// <summary>
    /// Setzt die Bounding-Box neu (Resize über Handles). Breite/Höhe
    /// werden auf ein Minimum von 0.1 mm begrenzt.
    /// </summary>
    public abstract void SetBounds(double x, double y, double width, double height);
}
