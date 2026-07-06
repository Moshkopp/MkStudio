using LuxiFer.Core.Canvas;

namespace LuxiFer.Core.Tests;

public class ShapeTests
{
    [Fact]
    public void Polyline_Bounds_umfasst_alle_Punkte()
    {
        var poly = new PolylineObject { Points = { (10, 20), (50, 5), (30, 40) } };
        var (x, y, w, h) = poly.Bounds;
        Assert.Equal(10, x);
        Assert.Equal(5, y);
        Assert.Equal(40, w);
        Assert.Equal(35, h);
    }

    [Fact]
    public void Polyline_SetBounds_skaliert_Punkte_proportional()
    {
        var poly = new PolylineObject { Points = { (0, 0), (10, 10) } };
        poly.SetBounds(5, 5, 20, 20);
        Assert.Equal((5, 5), poly.Points[0]);
        Assert.Equal((25, 25), poly.Points[1]);
    }

    [Fact]
    public void Line_MoveBy_verschiebt_beide_Endpunkte()
    {
        var line = new LineObject { X = 0, Y = 0, X2 = 10, Y2 = 10 };
        line.MoveBy(5, -5);
        Assert.Equal((5, -5, 15, 5), (line.X, line.Y, line.X2, line.Y2));
    }

    [Fact]
    public void SetBounds_erzwingt_Mindestgroesse()
    {
        var rect = new RectangleObject { X = 0, Y = 0, Width = 10, Height = 10 };
        rect.SetBounds(0, 0, 0, 0);
        Assert.Equal(0.1, rect.Width);
        Assert.Equal(0.1, rect.Height);
    }

    [Fact]
    public void HitTest_ueberspringt_gesperrte_Layer()
    {
        var doc = new CanvasDocument();
        var layer = Layer.CreateNext(0);
        layer.Locked = true;
        layer.Objects.Add(new RectangleObject { X = 0, Y = 0, Width = 10, Height = 10 });
        doc.Layers.Add(layer);
        Assert.Null(doc.HitTest(5, 5));
        layer.Locked = false;
        Assert.NotNull(doc.HitTest(5, 5));
    }

    [Fact]
    public void CreateNext_vergibt_Farben_reihum()
    {
        Assert.Equal(Layer.SwatchColors[0], Layer.CreateNext(0).ColorHex);
        Assert.Equal(Layer.SwatchColors[1], Layer.CreateNext(1).ColorHex);
        Assert.Equal(Layer.SwatchColors[0], Layer.CreateNext(Layer.SwatchColors.Length).ColorHex);
    }

    [Fact]
    public void Geschlossene_Formen_sind_fuellbar_offene_nicht()
    {
        Assert.True(new RectangleObject().IsFillable);
        Assert.True(new EllipseObject().IsFillable);
        Assert.False(new LineObject().IsFillable);
        Assert.False(new PolylineObject { Closed = false }.IsFillable);
        Assert.True(new PolylineObject { Closed = true }.IsFillable);
    }

    [Fact]
    public void Nur_Fill_und_Raster_Modus_werden_gefuellt()
    {
        Assert.False(LayerMode.Cut.IsFilled());
        Assert.True(LayerMode.Fill.IsFilled());
        Assert.True(LayerMode.Raster.IsFilled());
    }

    [Fact]
    public void RotatePoint_dreht_um_90_Grad_um_das_Zentrum()
    {
        // Punkt (10,0) um Zentrum (0,0) um 90° -> (0,10)
        var (x, y) = Geometry.RotatePoint(10, 0, 0, 0, 90);
        Assert.Equal(0, x, 6);
        Assert.Equal(10, y, 6);
    }

    [Fact]
    public void HitTest_beruecksichtigt_Rotation()
    {
        // Längliches Rechteck: 100 breit, 20 hoch, Zentrum bei (50,10).
        var rect = new RectangleObject { X = 0, Y = 0, Width = 100, Height = 20 };
        // Punkt oberhalb der Mitte, ungedreht außerhalb (y=45 > 20).
        Assert.False(rect.HitTest(50, 45));
        // Nach 90°-Drehung ragt das Rechteck vertikal (Halbhöhe 50) -> Treffer.
        rect.Rotation = 90;
        Assert.True(rect.HitTest(50, 45));
        // Punkt, der ungedreht getroffen hätte, liegt nun außerhalb.
        Assert.False(rect.HitTest(95, 10));
    }

    [Fact]
    public void Align_richtet_an_gemeinsamer_linker_Kante_aus()
    {
        var a = new RectangleObject { X = 10, Y = 0, Width = 20, Height = 10 };
        var b = new RectangleObject { X = 50, Y = 30, Width = 20, Height = 10 };
        var deltas = Arrange.Align([a, b], AlignKind.Left);
        // Linke Gruppen-Kante ist x=10; a bleibt, b wandert um -40.
        Assert.Equal((0.0, 0.0), deltas[0]);
        Assert.Equal((-40.0, 0.0), deltas[1]);
    }

    [Fact]
    public void Align_zentriert_horizontal_um_Gruppenmitte()
    {
        var a = new RectangleObject { X = 0, Y = 0, Width = 20, Height = 10 };
        var b = new RectangleObject { X = 80, Y = 0, Width = 20, Height = 10 };
        // Gruppe: x 0..100, Mitte 50. a-Mitte 10 -> +40; b-Mitte 90 -> -40.
        var deltas = Arrange.Align([a, b], AlignKind.HCenter);
        Assert.Equal(40.0, deltas[0].Dx, 6);
        Assert.Equal(-40.0, deltas[1].Dx, 6);
    }

    [Fact]
    public void Distribute_verteilt_Startkanten_gleichmaessig()
    {
        // Drei Objekte, Startkanten x=0, x=10, x=90. Erwartung: mittleres auf x=45.
        var a = new RectangleObject { X = 0, Y = 0, Width = 5, Height = 5 };
        var m = new RectangleObject { X = 10, Y = 0, Width = 5, Height = 5 };
        var z = new RectangleObject { X = 90, Y = 0, Width = 5, Height = 5 };
        var deltas = Arrange.Distribute([a, m, z], DistributeKind.Horizontal);
        // a und z bleiben stehen; m von 10 -> 45 = +35.
        Assert.Equal((0.0, 0.0), deltas[0]);
        Assert.Equal(35.0, deltas[1].Dx, 6);
        Assert.Equal((0.0, 0.0), deltas[2]);
    }

    [Fact]
    public void GroupBounds_umschliesst_alle_Objekte()
    {
        var a = new RectangleObject { X = 10, Y = 10, Width = 20, Height = 20 };
        var b = new RectangleObject { X = 50, Y = 5, Width = 10, Height = 40 };
        var (x, y, w, h) = Arrange.GroupBounds([a, b]);
        Assert.Equal(10, x);
        Assert.Equal(5, y);
        Assert.Equal(50, w);  // 60 - 10
        Assert.Equal(40, h);  // 45 - 5
    }
}
